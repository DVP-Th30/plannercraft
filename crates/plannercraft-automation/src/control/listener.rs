//! Écouteurs de l'application, dans un fil dédié, sur une socket Unix
//! (Linux, macOS) ou en TCP sur la boucle locale (Windows ; J8 lot L8,
//! C-CTL-11 à C-CTL-14, [`super::tcp`]) :
//!
//! - [`spawn_listener_in`] (J7, lot L3, ADR-0015 D9) : relais. Chaque
//!   requête authentifiée (`query`, `execute`, `ui_state`, `screenshot`)
//!   devient un [`RelayCall`] poussé dans la file de l'application, qui
//!   répond par son canal ; l'écouteur ne touche jamais la session ;
//! - [`spawn_hello_listener_in`] (J4, lot L10) : `hello` et `query`
//!   (`project.summary`) seulement, sans relais (garde du fichier ; gardé
//!   pour les tests des surfaces qui simulent une application).
//!
//! Chaque connexion commence par un `hello` porteur du jeton ; toute autre
//! requête avant, ou un jeton faux, est refusée et la connexion fermée
//! (C-CTL-01) ; une première ligne qui n'est pas une trame (requête HTTP
//! d'une page web, par exemple) ferme la connexion sans réponse (C-CTL-14).
//!
//! Transport : [`Transport::NATIVE`] par défaut ; `spawn_*_on` le choisit
//! (tests du transport TCP sous Linux). Le service est le même octet pour
//! octet sur les deux transports.

use std::fmt;
use std::sync::mpsc::SyncSender;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::{ControlError, RelayCall, Rendezvous, Transport, Waker, decode_token, state_dir};
use crate::view::SummaryView;

/// Capacité conseillée de la file des appels relayés (64) : au-delà,
/// l'écouteur répond `control.busy`.
pub const RELAY_QUEUE: usize = 64;

/// Délai d'un appel relayé, validation comprise (120 s, C-CTL-04).
pub const APPROVAL_TIMEOUT: Duration = Duration::from_secs(120);

/// Réglages de l'écouteur du relais.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ListenerOptions {
    /// Délai d'un appel depuis sa mise en file (validation comprise) ; à
    /// l'expiration, `control.approval_timeout` et l'appel est retiré.
    pub approval_timeout: Duration,
    /// Attente de la réponse d'un appel que l'application a pris juste
    /// avant l'expiration.
    pub reply_grace: Duration,
}

impl Default for ListenerOptions {
    fn default() -> Self {
        Self {
            approval_timeout: APPROVAL_TIMEOUT,
            reply_grace: Duration::from_secs(10),
        }
    }
}

/// Écouteur en cours ; arrêté à sa destruction (socket supprimée).
pub struct ListenerHandle {
    endpoint: String,
    token: String,
    running: imp::Running,
}

impl fmt::Debug for ListenerHandle {
    // Le jeton n'apparaît jamais dans une trace.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ListenerHandle")
            .field("endpoint", &self.endpoint)
            .field("token", &"…")
            .finish_non_exhaustive()
    }
}

impl ListenerHandle {
    /// Adresse de l'écouteur, sous la forme du rendez-vous : chemin de la
    /// socket Unix, ou `tcp:127.0.0.1:<port>`.
    #[must_use]
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    /// Rendez-vous à publier pour cet écouteur (`project` : chemin
    /// canonique, voir [`super::canonical_project`]).
    #[must_use]
    pub fn rendezvous(&self, project: Option<String>) -> Rendezvous {
        Rendezvous {
            v: super::PROTOCOL_VERSION,
            pid: std::process::id(),
            endpoint: self.endpoint.clone(),
            token: self.token.clone(),
            project,
        }
    }
}

impl ListenerHandle {
    /// Remplace le jeton de l'écouteur (relecture finale J7, relais F2 :
    /// un document neuf dans l'application). Les connexions déjà
    /// authentifiées se terminent normalement ; tout `hello` suivant doit
    /// porter le nouveau jeton, que [`ListenerHandle::rendezvous`] publie :
    /// un agent rattaché au document précédent est refusé
    /// (`control.bad_token`) au lieu d'agir en silence sur le nouveau.
    ///
    /// # Errors
    ///
    /// [`ControlError::Io`] si le générateur du système est indisponible
    /// (le jeton précédent reste).
    pub fn rotate_token(&mut self) -> Result<(), ControlError> {
        let token = super::new_token()?;
        let bytes = decode_token(&token).ok_or(ControlError::InvalidToken)?;
        self.running.set_token(bytes);
        self.token = token;
        Ok(())
    }
}

impl Drop for ListenerHandle {
    fn drop(&mut self) {
        self.running.stop();
    }
}

/// Lance l'écouteur `hello`/`query` dans un fil dédié, socket dans
/// [`state_dir`].
///
/// `summary` est le résumé du document ouvert, tenu à jour par
/// l'application (`None` : aucun document).
///
/// # Errors
///
/// [`ControlError::NoStateDir`], ou celles de [`spawn_hello_listener_in`].
pub fn spawn_hello_listener(
    token: String,
    summary: Arc<Mutex<Option<SummaryView>>>,
) -> Result<ListenerHandle, ControlError> {
    let dir = state_dir().ok_or(ControlError::NoStateDir)?;
    spawn_hello_listener_in(&dir, token, summary)
}

/// Lance l'écouteur sur le transport de la plateforme : socket
/// `<pid>-<n>.sock` (0600) dans `dir` (0700) sous Unix, `127.0.0.1:0`
/// ailleurs.
///
/// # Errors
///
/// Celles de [`spawn_hello_listener_on`].
pub fn spawn_hello_listener_in(
    dir: &std::path::Path,
    token: String,
    summary: Arc<Mutex<Option<SummaryView>>>,
) -> Result<ListenerHandle, ControlError> {
    spawn_hello_listener_on(dir, token, summary, Transport::NATIVE)
}

/// Lance l'écouteur J4 sur `transport` (`dir` ne sert qu'à la socket Unix).
///
/// # Errors
///
/// [`ControlError::InvalidToken`], [`ControlError::Invalid`] (chemin non
/// UTF-8), [`ControlError::Io`] (dossier, socket, port, fil) ;
/// [`ControlError::Unsupported`] pour une socket Unix hors Unix.
pub fn spawn_hello_listener_on(
    dir: &std::path::Path,
    token: String,
    summary: Arc<Mutex<Option<SummaryView>>>,
    transport: Transport,
) -> Result<ListenerHandle, ControlError> {
    let expected = decode_token(&token).ok_or(ControlError::InvalidToken)?;
    let (endpoint, running) = imp::spawn(dir, expected, imp::Service::Hello(summary), transport)?;
    Ok(ListenerHandle {
        endpoint,
        token,
        running,
    })
}

/// Écouteur du relais J7 (ADR-0015 D9, lot L3), réglages par défaut
/// ([`ListenerOptions`]) : voir [`spawn_listener_with`].
///
/// # Errors
///
/// Celles de [`spawn_listener_with`].
pub fn spawn_listener_in(
    dir: &std::path::Path,
    token: String,
    inbox: SyncSender<RelayCall>,
    wake: Waker,
) -> Result<ListenerHandle, ControlError> {
    spawn_listener_with(dir, token, inbox, wake, ListenerOptions::default())
}

/// Lance l'écouteur du relais sur le transport de la plateforme (socket
/// `<pid>-<n>.sock` (0600) dans `dir` (0700) sous Unix, `127.0.0.1:0`
/// ailleurs) : voir [`spawn_listener_on`].
///
/// # Errors
///
/// Celles de [`spawn_listener_on`].
pub fn spawn_listener_with(
    dir: &std::path::Path,
    token: String,
    inbox: SyncSender<RelayCall>,
    wake: Waker,
    options: ListenerOptions,
) -> Result<ListenerHandle, ControlError> {
    spawn_listener_on(dir, token, inbox, wake, options, Transport::NATIVE)
}

/// Lance l'écouteur du relais sur `transport` (`dir` ne sert qu'à la
/// socket Unix) : chaque requête authentifiée devient un [`RelayCall`] poussé
/// dans `inbox` (file bornée de l'application ; pleine : `control.busy`,
/// connexion maintenue), puis `wake` réveille l'interface. Les réponses de
/// l'application (`pending_approval`, puis `ok` ou `err`) partent sur la
/// même connexion. Sans réponse finale en `options.approval_timeout`,
/// l'appel expire ([`super::CallState::expire`]), un `Cancel(id)` suit
/// dans la file et l'agent reçoit `control.approval_timeout`.
///
/// # Errors
///
/// [`ControlError::InvalidToken`], [`ControlError::Invalid`] (chemin non
/// UTF-8), [`ControlError::Io`] (dossier, socket, port, fil) ;
/// [`ControlError::Unsupported`] pour une socket Unix hors Unix.
pub fn spawn_listener_on(
    dir: &std::path::Path,
    token: String,
    inbox: SyncSender<RelayCall>,
    wake: Waker,
    options: ListenerOptions,
    transport: Transport,
) -> Result<ListenerHandle, ControlError> {
    let expected = decode_token(&token).ok_or(ControlError::InvalidToken)?;
    let service = imp::Service::Relay { inbox, wake, options };
    let (endpoint, running) = imp::spawn(dir, expected, service, transport)?;
    Ok(ListenerHandle {
        endpoint,
        token,
        running,
    })
}

mod imp {
    use std::collections::VecDeque;
    use std::io::{BufRead, BufReader, ErrorKind, Write};
    use std::net::{SocketAddr, SocketAddrV4, TcpListener};
    #[cfg(unix)]
    use std::os::unix::net::{UnixListener, UnixStream};
    use std::path::Path;
    #[cfg(unix)]
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
    use std::sync::mpsc::{RecvTimeoutError, SyncSender, TrySendError, sync_channel};
    use std::sync::{Arc, Mutex, PoisonError};
    use std::thread::{self, JoinHandle};
    use std::time::{Duration, Instant};

    use serde_json::{Map, Value, json};

    use super::super::tcp::{accept_peer, bind_loopback, local_endpoint};
    use super::super::{
        CallState, ControlError, ControlStream, Endpoint, Frame, LineError, MAX_LINE_BYTES, Method,
        PROTOCOL_VERSION, RelayCall, RelayKind, Reply, Transport, Waker, codes, read_line_bounded,
        read_line_bounded_with, to_line, token_matches,
    };
    #[cfg(unix)]
    use super::super::{ensure_dir, next_stem};
    use super::ListenerOptions;
    use crate::error::codes as tool_codes;
    use crate::registry::{Kind, lookup};
    use crate::view::SummaryView;

    /// Connexions servies en même temps ; au-delà, refusées par
    /// `control.busy` (le client ne se détache pas).
    const MAX_CONNECTIONS: usize = 8;
    /// Connexion authentifiée sans requête fermée après ce délai.
    const IDLE_TIMEOUT: Duration = Duration::from_secs(30);
    /// Connexion fermée si son `hello` n'arrive pas dans ce délai (une place
    /// sur huit n'est pas tenue 30 s par une connexion muette). Échéance
    /// absolue depuis l'acceptation (relecture finale de J8, sécurité F1) :
    /// une ligne envoyée octet par octet ne la repousse pas.
    const HELLO_TIMEOUT: Duration = Duration::from_secs(5);
    /// Octets d'une ligne avant le `hello` accepté (un `hello` fait moins
    /// de 200 octets) : une connexion sans jeton ne retient pas 8 Mio.
    const MAX_HELLO_BYTES: usize = 16 * 1024;
    /// Connexions sans `hello` accepté au plus ; au-delà, la plus ancienne
    /// est évincée : elles ne peuvent jamais prendre toutes les places
    /// (relecture finale de J8, sécurité F1 : sous TCP, un autre compte de
    /// la machine pouvait bloquer le relais par 8 connexions muettes).
    const MAX_PENDING: usize = 4;
    /// Délai d'écriture du refus d'une connexion évincée (le fil
    /// d'acceptation ne s'y attarde pas).
    const EVICT_WRITE_TIMEOUT: Duration = Duration::from_millis(200);
    /// Tranche de lecture avant le `hello` : une connexion évincée voit
    /// son éviction en au plus ce délai et rend sa place. Sous Windows,
    /// la coupure (`shutdown`) par le fil d'acceptation ne réveille pas la
    /// lecture bloquée de son fil (CI Windows de la PR #7 : les évincées
    /// gardaient leur place jusqu'à l'échéance du `hello`, le client muni
    /// du jeton recevait `control.busy`).
    const EVICT_POLL: Duration = Duration::from_millis(50);
    /// Pas de la surveillance de la connexion d'un appel en attente : un
    /// client parti (délai écoulé, processus mort) est vu en au plus ce
    /// délai, et son appel expiré.
    const HANGUP_POLL: Duration = Duration::from_millis(100);
    /// Délai d'écriture d'une réponse.
    const WRITE_TIMEOUT: Duration = Duration::from_secs(5);
    /// Lecture servie par défaut (`query` sans `command`).
    const SUMMARY_COMMAND: &str = "project.summary";

    /// Ce que sert l'écouteur.
    pub(super) enum Service {
        /// J4 : `hello` et le résumé tenu par l'application.
        Hello(Arc<Mutex<Option<SummaryView>>>),
        /// J7 : relais vers la file de l'application.
        Relay {
            /// File de l'application.
            inbox: SyncSender<RelayCall>,
            /// Réveil de l'interface.
            wake: Waker,
            /// Délais.
            options: ListenerOptions,
        },
    }

    struct Shared {
        stop: AtomicBool,
        /// Jeton attendu, remplacé par `ListenerHandle::rotate_token`.
        token: Mutex<[u8; 32]>,
        service: Service,
        active: AtomicUsize,
        /// Connexions sans `hello` accepté, de la plus ancienne à la plus
        /// récente (numéro, second descripteur pour l'éviction, marque
        /// d'éviction lue par le fil de la connexion).
        pending: Mutex<VecDeque<(u64, ControlStream, Arc<AtomicBool>)>>,
        /// Numéro de la prochaine connexion.
        next_conn: AtomicU64,
    }

    impl Shared {
        /// Inscrit la connexion `id` parmi celles sans `hello` ; au-delà de
        /// [`MAX_PENDING`], la plus ancienne est coupée (sa lecture se
        /// termine, son fil rend la place). Rend la marque d'éviction de
        /// la connexion.
        fn add_pending(&self, id: u64, stream: &ControlStream) -> Arc<AtomicBool> {
            let flag = Arc::new(AtomicBool::new(false));
            let Ok(clone) = stream.try_clone() else {
                return flag;
            };
            let evicted = {
                let mut pending = self.pending.lock().unwrap_or_else(PoisonError::into_inner);
                pending.push_back((id, clone, Arc::clone(&flag)));
                if pending.len() > MAX_PENDING {
                    pending.pop_front()
                } else {
                    None
                }
            };
            if let Some((_, old, old_flag)) = evicted {
                // Refus dit comme pour des connexions pleines : un client
                // légitime évincé avant son `hello` lit `control.busy` et
                // ne se détache pas ; puis coupure, qui termine la lecture
                // du fil de la connexion.
                let busy = Reply::error(0, codes::BUSY, "trop de connexions sans hello : réessayer");
                if old.set_write_timeout(Some(EVICT_WRITE_TIMEOUT)).is_ok() {
                    let _ = send(&old, &busy);
                }
                // Marque posée après le refus : le fil de la connexion ne
                // la ferme pas avant que `control.busy` soit écrit.
                old_flag.store(true, Ordering::SeqCst);
                old.shutdown();
            }
            flag
        }

        /// Retire la connexion `id` des connexions sans `hello`.
        fn drop_pending(&self, id: u64) {
            let mut pending = self.pending.lock().unwrap_or_else(PoisonError::into_inner);
            pending.retain(|(other, _, _)| *other != id);
        }
    }

    /// Écouteur lié.
    enum Bound {
        /// Socket Unix et son chemin.
        #[cfg(unix)]
        Unix(UnixListener, PathBuf),
        /// Port en boucle locale.
        Tcp(TcpListener, SocketAddrV4),
    }

    /// Adresse à joindre pour réveiller `accept`, et fichier à supprimer.
    enum Wake {
        #[cfg(unix)]
        Unix(PathBuf),
        Tcp(SocketAddrV4),
    }

    pub(super) struct Running {
        shared: Arc<Shared>,
        wake: Wake,
        thread: Option<JoinHandle<()>>,
    }

    impl Running {
        pub(super) fn set_token(&self, token: [u8; 32]) {
            *self.shared.token.lock().unwrap_or_else(PoisonError::into_inner) = token;
        }

        pub(super) fn stop(&mut self) {
            /// Attente de la connexion de réveil (TCP).
            const WAKE_TIMEOUT: Duration = Duration::from_secs(1);
            self.shared.stop.store(true, Ordering::SeqCst);
            // Réveille `accept`. Écouteur injoignable (socket supprimée,
            // dossier retiré) : le fil n'est pas attendu, pour ne jamais
            // bloquer la fermeture ; il reste suspendu sur `accept`.
            let woken = match &self.wake {
                #[cfg(unix)]
                Wake::Unix(path) => UnixStream::connect(path).is_ok(),
                Wake::Tcp(addr) => Endpoint::Tcp(*addr).connect(WAKE_TIMEOUT).is_ok(),
            };
            if let Some(thread) = self.thread.take()
                && (woken || thread.is_finished())
            {
                let _ = thread.join();
            }
            #[cfg(unix)]
            if let Wake::Unix(path) = &self.wake {
                let _ = std::fs::remove_file(path);
            }
        }
    }

    /// Lie l'écouteur : socket `<pid>-<n>.sock` (0600) dans `dir` (0700),
    /// ou `127.0.0.1:0` ; rend l'adresse sous la forme du rendez-vous.
    fn bind(dir: &Path, transport: Transport) -> Result<(String, Bound), ControlError> {
        match transport {
            Transport::Tcp => {
                let _ = dir;
                let listener = bind_loopback()?;
                let addr = local_endpoint(&listener)?;
                Ok((Endpoint::Tcp(addr).to_rendezvous(), Bound::Tcp(listener, addr)))
            }
            #[cfg(unix)]
            Transport::Unix => {
                use std::os::unix::fs::PermissionsExt;

                ensure_dir(dir)?;
                let socket = dir.join(format!("{}.sock", next_stem()));
                let endpoint = socket
                    .to_str()
                    .ok_or_else(|| ControlError::Invalid("chemin de socket non UTF-8".to_owned()))?
                    .to_owned();
                // Reste éventuel d'un processus mort au même pid.
                let _ = std::fs::remove_file(&socket);
                let listener = UnixListener::bind(&socket)
                    .map_err(|e| ControlError::io("ouverture de la socket", &e))?;
                if let Err(e) = std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600)) {
                    let _ = std::fs::remove_file(&socket);
                    return Err(ControlError::io("droits de la socket", &e));
                }
                Ok((endpoint, Bound::Unix(listener, socket)))
            }
            #[cfg(not(unix))]
            Transport::Unix => {
                let _ = dir;
                Err(ControlError::Unsupported)
            }
        }
    }

    pub(super) fn spawn(
        dir: &Path,
        token: [u8; 32],
        service: Service,
        transport: Transport,
    ) -> Result<(String, Running), ControlError> {
        let (endpoint, bound) = bind(dir, transport)?;
        let wake = match &bound {
            #[cfg(unix)]
            Bound::Unix(_, path) => Wake::Unix(path.clone()),
            Bound::Tcp(_, addr) => Wake::Tcp(*addr),
        };
        let shared = Arc::new(Shared {
            stop: AtomicBool::new(false),
            token: Mutex::new(token),
            service,
            active: AtomicUsize::new(0),
            pending: Mutex::new(VecDeque::new()),
            next_conn: AtomicU64::new(0),
        });
        let worker = Arc::clone(&shared);
        let thread = thread::Builder::new()
            .name("plannercraft-control".to_owned())
            .spawn(move || accept_loop(&bound, &worker));
        match thread {
            Ok(thread) => Ok((
                endpoint,
                Running {
                    shared,
                    wake,
                    thread: Some(thread),
                },
            )),
            Err(e) => {
                #[cfg(unix)]
                if let Wake::Unix(path) = &wake {
                    let _ = std::fs::remove_file(path);
                }
                Err(ControlError::io("fil de l'écouteur", &e))
            }
        }
    }

    /// Connexion acceptée ; `None` : pair refusé (hors boucle locale,
    /// C-CTL-12), fermé sans lecture.
    fn accept(bound: &Bound) -> std::io::Result<Option<ControlStream>> {
        match bound {
            #[cfg(unix)]
            Bound::Unix(listener, _) => listener.accept().map(|(s, _)| Some(ControlStream::from_unix(s))),
            Bound::Tcp(listener, _) => {
                let (stream, peer): (_, SocketAddr) = listener.accept()?;
                if !accept_peer(&peer) {
                    // Fermé aussitôt : aucun octet lu, aucune réponse.
                    drop(stream);
                    return Ok(None);
                }
                let _ = stream.set_nodelay(true);
                Ok(Some(ControlStream::from_tcp(stream)))
            }
        }
    }

    /// Décrémente le compteur de connexions à la fin du fil.
    struct ActiveGuard(Arc<Shared>);

    impl Drop for ActiveGuard {
        fn drop(&mut self) {
            self.0.active.fetch_sub(1, Ordering::SeqCst);
        }
    }

    fn accept_loop(bound: &Bound, shared: &Arc<Shared>) {
        loop {
            let conn = accept(bound);
            if shared.stop.load(Ordering::SeqCst) {
                break;
            }
            let stream = match conn {
                Ok(Some(stream)) => stream,
                Ok(None) => continue,
                Err(_) => {
                    // Descripteurs épuisés… : on laisse respirer le système.
                    thread::sleep(Duration::from_millis(20));
                    continue;
                }
            };
            if shared.active.fetch_add(1, Ordering::SeqCst) >= MAX_CONNECTIONS {
                shared.active.fetch_sub(1, Ordering::SeqCst);
                // Refus dit au client (relecture finale J7, relais F4) :
                // une fermeture muette se lisait « application partie » et
                // détachait l'agent d'une application vivante.
                let busy = Reply::error(
                    0,
                    codes::BUSY,
                    format!("{MAX_CONNECTIONS} connexions déjà servies : réessayer"),
                );
                if stream.set_write_timeout(Some(WRITE_TIMEOUT)).is_ok() {
                    let _ = send(&stream, &busy);
                }
                stream.close_gracefully();
                continue;
            }
            let id = shared.next_conn.fetch_add(1, Ordering::SeqCst);
            let evicted = shared.add_pending(id, &stream);
            let guard = ActiveGuard(Arc::clone(shared));
            // Échec du fil : la fermeture de `guard` rend la place.
            let spawned = thread::Builder::new()
                .name("plannercraft-control-conn".to_owned())
                .spawn(move || {
                    serve(&stream, &guard.0, id, &evicted);
                    guard.0.drop_pending(id);
                    stream.close_gracefully();
                    drop(guard);
                });
            if spawned.is_err() {
                shared.drop_pending(id);
            }
        }
    }

    /// Suite d'une requête lue.
    enum Step {
        /// Réponse immédiate, et si la connexion reste ouverte.
        Reply(Reply, bool),
        /// Fermeture sans réponse (première ligne qui n'est pas une trame,
        /// C-CTL-14).
        Close,
        /// Appel à relayer à l'application, avec le délai de validation
        /// demandé par le client (`approval_timeout_ms`), s'il y en a un.
        Relay(u64, RelayKind, Option<Duration>),
    }

    fn serve(stream: &ControlStream, shared: &Shared, id: u64, evicted: &AtomicBool) {
        if stream.set_read_timeout(Some(HELLO_TIMEOUT)).is_err()
            || stream.set_write_timeout(Some(WRITE_TIMEOUT)).is_err()
        {
            return;
        }
        let hello_deadline = Instant::now()
            .checked_add(HELLO_TIMEOUT)
            .unwrap_or_else(Instant::now);
        let mut reader = BufReader::new(stream);
        let mut authenticated = false;
        loop {
            let read = if authenticated {
                // `hello` accepté : délai d'une connexion inactive.
                if stream.set_read_timeout(Some(IDLE_TIMEOUT)).is_err() {
                    return;
                }
                read_line_bounded(&mut reader, MAX_LINE_BYTES)
            } else {
                // Avant le `hello` : échéance absolue, ligne courte, lue
                // par tranches de `EVICT_POLL` (éviction vue sous Windows).
                let mut before_fill = || {
                    if evicted.load(Ordering::SeqCst) {
                        return Err(std::io::Error::from(ErrorKind::ConnectionAborted));
                    }
                    let left = hello_deadline.saturating_duration_since(Instant::now());
                    if left.is_zero() {
                        return Err(std::io::Error::from(ErrorKind::TimedOut));
                    }
                    stream.set_read_timeout(Some(left.min(EVICT_POLL)))
                };
                read_line_bounded_with(&mut reader, MAX_HELLO_BYTES, &mut before_fill)
            };
            let line = match read {
                Ok(Some(line)) => line,
                Ok(None) | Err(LineError::Io(_)) => return,
                // Avant le `hello`, rien n'est répondu à ce qui n'est pas
                // une trame (C-CTL-14).
                Err(LineError::TooLong) if !authenticated => return,
                Err(LineError::TooLong) => {
                    let reply = Reply::error(
                        0,
                        codes::LINE_TOO_LONG,
                        format!("ligne de plus de {MAX_LINE_BYTES} octets"),
                    );
                    let _ = send(stream, &reply);
                    return;
                }
            };
            if shared.stop.load(Ordering::SeqCst) {
                return;
            }
            let was_authenticated = authenticated;
            let step = respond(&line, &mut authenticated, shared);
            if authenticated && !was_authenticated {
                shared.drop_pending(id);
            }
            match step {
                Step::Reply(reply, keep_open) => {
                    if send(stream, &reply).is_err() || !keep_open {
                        return;
                    }
                }
                Step::Close => return,
                Step::Relay(id, kind, asked) => {
                    if relay(&mut reader, shared, id, kind, asked).is_err() {
                        return;
                    }
                }
            }
        }
    }

    fn send(mut stream: &ControlStream, reply: &Reply) -> std::io::Result<()> {
        let line = to_line(reply).map_err(std::io::Error::other)?;
        stream.write_all(&line)
    }

    /// Réponse à une ligne, ou appel à relayer.
    fn respond(line: &[u8], authenticated: &mut bool, shared: &Shared) -> Step {
        let frame = match serde_json::from_slice::<Frame>(line) {
            Ok(frame) => frame,
            // Première ligne qui n'est pas une trame (requête HTTP d'une
            // page web, octets quelconques) : fermée sans réponse, rien
            // n'est dit à qui ne parle pas le protocole (C-CTL-14).
            Err(_) if !*authenticated => return Step::Close,
            Err(e) => {
                // L'identifiant est repris s'il se lit encore.
                let id = serde_json::from_slice::<Value>(line)
                    .ok()
                    .and_then(|v| v.get("id").and_then(Value::as_u64))
                    .unwrap_or(0);
                let reply = Reply::error(id, codes::INVALID_FRAME, format!("trame illisible : {e}"));
                return Step::Reply(reply, *authenticated);
            }
        };
        let id = frame.id;
        if frame.v != PROTOCOL_VERSION {
            let reply = Reply::error(
                id,
                codes::UNSUPPORTED_VERSION,
                format!(
                    "version {} du protocole non prise en charge (attendue : {PROTOCOL_VERSION})",
                    frame.v
                ),
            );
            return Step::Reply(reply, *authenticated);
        }
        if frame.method == Method::Hello {
            let given = frame.params.get("token").and_then(Value::as_str).unwrap_or("");
            let expected = *shared.token.lock().unwrap_or_else(PoisonError::into_inner);
            return if token_matches(&expected, given) {
                *authenticated = true;
                let result = hello_result(&shared.service);
                Step::Reply(Reply::Ok { id, result }, true)
            } else {
                *authenticated = false;
                Step::Reply(Reply::error(id, codes::BAD_TOKEN, "jeton refusé"), false)
            };
        }
        if !*authenticated {
            return Step::Reply(
                Reply::error(id, codes::UNAUTHENTICATED, "hello avec jeton attendu en premier"),
                false,
            );
        }
        match &shared.service {
            Service::Hello(summary) => match frame.method {
                Method::Query => Step::Reply(query_summary(id, &frame.params, summary), true),
                other => Step::Reply(
                    Reply::error(
                        id,
                        codes::UNSUPPORTED,
                        format!(
                            "méthode « {} » non prise en charge par cet écouteur",
                            other.wire_name()
                        ),
                    ),
                    true,
                ),
            },
            Service::Relay { .. } => match relay_kind(frame.method, frame.params) {
                Ok((kind, asked)) => Step::Relay(id, kind, asked),
                Err((code, message)) => Step::Reply(Reply::error(id, code, message), true),
            },
        }
    }

    fn hello_result(service: &Service) -> Value {
        let relay = matches!(service, Service::Relay { .. });
        let methods: Vec<&str> = Method::ALL
            .iter()
            .filter(|m| relay || m.available_in_j4())
            .map(|m| m.wire_name())
            .collect();
        json!({
            "v": PROTOCOL_VERSION,
            "app": plannercraft_core::PRODUCT_ID,
            "version": env!("CARGO_PKG_VERSION"),
            "pid": std::process::id(),
            "methods": methods,
        })
    }

    /// Paramètres en objet (`null` : objet vide) ; sinon refus.
    fn object(params: Value) -> Result<Map<String, Value>, (&'static str, String)> {
        match params {
            Value::Null => Ok(Map::new()),
            Value::Object(map) => Ok(map),
            _ => Err((codes::INVALID_FRAME, "« params » doit être un objet".to_owned())),
        }
    }

    /// Refuse les clés restantes.
    fn no_more(map: &Map<String, Value>) -> Result<(), (&'static str, String)> {
        match map.keys().next() {
            None => Ok(()),
            Some(k) => Err((codes::INVALID_FRAME, format!("champ inconnu « {k} »"))),
        }
    }

    /// Délai de validation demandé par le client (`approval_timeout_ms`,
    /// entier positif), retiré de `map`.
    fn asked_timeout(map: &mut Map<String, Value>) -> Result<Option<Duration>, (&'static str, String)> {
        match map.remove("approval_timeout_ms") {
            None | Some(Value::Null) => Ok(None),
            Some(v) => v
                .as_u64()
                .filter(|ms| *ms > 0)
                .map(|ms| Some(Duration::from_millis(ms)))
                .ok_or((
                    codes::INVALID_FRAME,
                    "« approval_timeout_ms » doit être un entier positif".to_owned(),
                )),
        }
    }

    /// Appel relayé d'une requête authentifiée, et le délai de validation
    /// demandé (`execute` seulement).
    fn relay_kind(
        method: Method,
        params: Value,
    ) -> Result<(RelayKind, Option<Duration>), (&'static str, String)> {
        let mut map = object(params)?;
        match method {
            // Traité avant.
            Method::Hello => Err((codes::INVALID_FRAME, "hello déjà traité".to_owned())),
            Method::Query => {
                let tool = match map.remove("command") {
                    None | Some(Value::Null) => SUMMARY_COMMAND.to_owned(),
                    Some(Value::String(c)) => c,
                    Some(_) => {
                        return Err((
                            codes::INVALID_FRAME,
                            "« command » doit être une chaîne".to_owned(),
                        ));
                    }
                };
                let params = map.remove("params").unwrap_or_else(|| Value::Object(Map::new()));
                no_more(&map)?;
                let readable = lookup(&tool).is_some_and(|s| matches!(s.kind, Kind::Read | Kind::Analysis));
                if !readable {
                    return Err((
                        codes::UNSUPPORTED,
                        format!("« {tool} » n'est pas une lecture : passer par execute"),
                    ));
                }
                Ok((RelayKind::Execute { tool, params }, None))
            }
            Method::Execute => {
                let Some(Value::String(tool)) = map.remove("tool") else {
                    return Err((codes::INVALID_FRAME, "« tool » (chaîne) attendu".to_owned()));
                };
                let params = map.remove("params").unwrap_or_else(|| Value::Object(Map::new()));
                let asked = asked_timeout(&mut map)?;
                no_more(&map)?;
                Ok((RelayKind::Execute { tool, params }, asked))
            }
            Method::UiState => {
                no_more(&map)?;
                Ok((RelayKind::UiState, None))
            }
            Method::Screenshot => {
                let scale_down = match map.remove("scale_down") {
                    None | Some(Value::Null) => 0,
                    Some(v) => v.as_u64().map(|n| u8::try_from(n).unwrap_or(u8::MAX)).ok_or((
                        codes::INVALID_FRAME,
                        "« scale_down » doit être un entier".to_owned(),
                    ))?,
                };
                no_more(&map)?;
                Ok((RelayKind::Screenshot { scale_down }, None))
            }
        }
    }

    /// Relaie un appel à l'application et transmet ses réponses ; rend
    /// l'erreur d'écriture sur la connexion (connexion à fermer). Délai :
    /// celui de l'écouteur, ou celui demandé par le client s'il est plus
    /// court (relecture finale J7, relais F1 : sinon l'agent apprenait
    /// « rien n'a été appliqué » alors que le bandeau restait acceptable).
    /// Pendant l'attente, la connexion est surveillée : client parti ⇒
    /// appel expiré, bandeau retiré.
    fn relay(
        reader: &mut BufReader<&ControlStream>,
        shared: &Shared,
        id: u64,
        kind: RelayKind,
        asked: Option<Duration>,
    ) -> std::io::Result<()> {
        let stream = *reader.get_ref();
        let Service::Relay { inbox, wake, options } = &shared.service else {
            return send(stream, &Reply::error(id, codes::UNSUPPORTED, "relais non servi"));
        };
        let (tx, rx) = sync_channel::<Reply>(4);
        let state = Arc::new(CallState::new());
        let call = RelayCall {
            id,
            kind,
            state: Arc::clone(&state),
            reply: tx,
        };
        match inbox.try_send(call) {
            Ok(()) => wake(),
            Err(TrySendError::Full(_)) => {
                return send(
                    stream,
                    &Reply::error(
                        id,
                        codes::BUSY,
                        "file des appels de l'application pleine : réessayer",
                    ),
                );
            }
            Err(TrySendError::Disconnected(_)) => {
                return send(
                    stream,
                    &Reply::error(
                        id,
                        tool_codes::CONTROL_APP_GONE,
                        "application en cours de fermeture",
                    ),
                );
            }
        }
        let timeout = asked.map_or(options.approval_timeout, |asked| {
            asked.min(options.approval_timeout)
        });
        let start = Instant::now();
        let mut pending = false;
        loop {
            let left = timeout.saturating_sub(start.elapsed());
            match rx.recv_timeout(left.min(HANGUP_POLL)) {
                Ok(reply @ Reply::PendingApproval { .. }) => {
                    pending = true;
                    send(stream, &reply)?;
                }
                Ok(reply) => return send(stream, &reply),
                Err(RecvTimeoutError::Disconnected) => {
                    return send(
                        stream,
                        &Reply::error(id, codes::INTERNAL, "appel abandonné par l'application"),
                    );
                }
                Err(RecvTimeoutError::Timeout) if !left.is_zero() && left > HANGUP_POLL => {
                    if hung_up(reader) {
                        // Client parti : plus personne à qui répondre.
                        if state.expire() {
                            cancel(inbox, wake, id, &state);
                        }
                        return Err(std::io::Error::from(ErrorKind::ConnectionAborted));
                    }
                }
                Err(RecvTimeoutError::Timeout) => {
                    if state.expire() {
                        cancel(inbox, wake, id, &state);
                        return send(stream, &expired(id, pending));
                    }
                    // Pris juste avant l'expiration : la réponse suit.
                    return match rx.recv_timeout(options.reply_grace) {
                        Ok(Reply::PendingApproval { .. }) | Err(_) => send(stream, &expired(id, pending)),
                        Ok(reply) => send(stream, &reply),
                    };
                }
            }
        }
    }

    /// Vrai si le client a fermé la connexion (fin de flux), lu sans
    /// attendre ; un octet reçu reste dans le tampon du lecteur.
    fn hung_up(reader: &mut BufReader<&ControlStream>) -> bool {
        let stream = *reader.get_ref();
        if stream.set_nonblocking(true).is_err() {
            return false;
        }
        let gone = match reader.fill_buf() {
            Ok(buf) => buf.is_empty(),
            Err(e) => !matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::Interrupted),
        };
        let _ = stream.set_nonblocking(false);
        gone
    }

    /// Réponse d'un appel expiré : validation attendue (`approval_timeout`)
    /// ou application muette.
    fn expired(id: u64, pending: bool) -> Reply {
        if pending {
            Reply::error(
                id,
                tool_codes::CONTROL_APPROVAL_TIMEOUT,
                "aucune réponse de l'utilisateur dans le délai : rien n'a été appliqué",
            )
        } else {
            Reply::error(id, tool_codes::CONTROL_APP_GONE, "l'application ne répond pas")
        }
    }

    /// Signale à l'application l'abandon de l'appel `id` (bandeau retiré) ;
    /// le `state` de l'appel abandonné l'accompagne : l'application ne
    /// retire que cet appel-là, pas celui d'un autre agent au même `id`
    /// (relecture finale J7, relais F3). File pleine ou fermée :
    /// l'application voit l'appel expiré à la prochaine image
    /// ([`CallState::is_pending`]).
    fn cancel(inbox: &SyncSender<RelayCall>, wake: &Waker, id: u64, state: &Arc<CallState>) {
        let (reply, _) = sync_channel(1);
        let call = RelayCall {
            id,
            kind: RelayKind::Cancel(id),
            state: Arc::clone(state),
            reply,
        };
        if inbox.try_send(call).is_ok() {
            wake();
        }
    }

    /// `query` du J4 : `params` nul, `{}` ou `{"command":"project.summary"}`.
    fn query_summary(id: u64, params: &Value, summary: &Mutex<Option<SummaryView>>) -> Reply {
        let command = match params {
            Value::Null => None,
            Value::Object(map) => match map.get("command") {
                None => None,
                Some(Value::String(c)) => Some(c.as_str()),
                Some(_) => {
                    return Reply::error(id, codes::INVALID_FRAME, "« command » doit être une chaîne");
                }
            },
            _ => return Reply::error(id, codes::INVALID_FRAME, "« params » doit être un objet"),
        };
        if let Some(c) = command.filter(|c| *c != SUMMARY_COMMAND) {
            return Reply::error(
                id,
                codes::UNSUPPORTED,
                format!("lecture « {c} » non prise en charge par cet écouteur (seule : {SUMMARY_COMMAND})"),
            );
        }
        let summary = summary.lock().unwrap_or_else(PoisonError::into_inner).clone();
        match serde_json::to_value(summary) {
            Ok(result) => Reply::Ok { id, result },
            Err(e) => Reply::error(id, codes::INTERNAL, format!("résumé illisible : {e}")),
        }
    }
}
