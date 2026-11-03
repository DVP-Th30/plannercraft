//! Adresse du canal de contrôle dans le rendez-vous (C-CTL-11, ADR-0016
//! D12) : `unix:<chemin>` (socket Unix) ou `tcp:127.0.0.1:<port>` (boucle
//! locale, Windows). Contrat posé par le lot L0, adopté par le rendez-vous,
//! la sonde et le client au lot L8 : [`Endpoint::connect`] est la connexion
//! commune aux deux transports, [`ControlStream`] le flux qui en sort.
//!
//! Forme écrite dans le rendez-vous ([`Endpoint::to_rendezvous`]) : une
//! socket Unix garde le chemin nu du J7 (octets du rendez-vous inchangés
//! sous Linux et macOS) ; TCP s'écrit `tcp:127.0.0.1:<port>`. La lecture
//! ([`Endpoint::from_rendezvous`]) prend les deux formes.

use std::io::{self, Read, Write};
use std::net::{Shutdown, SocketAddr, SocketAddrV4, TcpStream};
#[cfg(unix)]
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

use super::ControlError;

/// Transport d'un écouteur.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Transport {
    /// Socket Unix (Linux, macOS).
    Unix,
    /// TCP en boucle locale (`127.0.0.1:0`), compilé et testé partout.
    Tcp,
}

impl Transport {
    /// Transport de la plateforme : socket Unix sous Unix, TCP ailleurs
    /// (Windows).
    pub const NATIVE: Self = if cfg!(unix) { Self::Unix } else { Self::Tcp };
}

/// Adresse d'écoute de l'application.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Endpoint {
    /// Socket Unix.
    Unix(PathBuf),
    /// TCP en boucle locale (IPv4 seulement).
    Tcp(SocketAddrV4),
}

impl Endpoint {
    /// Lecture de `unix:<chemin>` ou `tcp:<ipv4>:<port>` ; un hôte TCP qui
    /// n'est pas en boucle locale est refusé (C-CTL-11).
    ///
    /// # Errors
    ///
    /// [`ControlError::BadEndpoint`] (`control.bad_endpoint`).
    pub fn parse(s: &str) -> Result<Self, ControlError> {
        let bad = || ControlError::BadEndpoint(s.chars().take(200).collect());
        if let Some(path) = s.strip_prefix("unix:") {
            if path.is_empty() || path.contains('\0') {
                return Err(bad());
            }
            return Ok(Self::Unix(PathBuf::from(path)));
        }
        if let Some(addr) = s.strip_prefix("tcp:") {
            let addr: SocketAddrV4 = addr.parse().map_err(|_| bad())?;
            if !addr.ip().is_loopback() || addr.port() == 0 {
                return Err(bad());
            }
            return Ok(Self::Tcp(addr));
        }
        Err(bad())
    }

    /// Écriture (inverse de [`Endpoint::parse`]) ; un chemin non UTF-8 est
    /// écrit avec des caractères de remplacement (relu différemment, donc
    /// refusé au rattachement plutôt que mal interprété).
    #[must_use]
    pub fn render(&self) -> String {
        match self {
            Self::Unix(path) => format!("unix:{}", path.to_string_lossy()),
            Self::Tcp(addr) => format!("tcp:{addr}"),
        }
    }

    /// Lecture du champ `endpoint` d'un rendez-vous : `unix:`/`tcp:` comme
    /// [`Endpoint::parse`], ou chemin absolu nu (socket Unix, forme J7).
    ///
    /// # Errors
    ///
    /// [`ControlError::BadEndpoint`] (`control.bad_endpoint`).
    pub fn from_rendezvous(s: &str) -> Result<Self, ControlError> {
        if s.starts_with("unix:") || s.starts_with("tcp:") {
            return Self::parse(s);
        }
        if s.contains('\0') || !Path::new(s).is_absolute() {
            return Err(ControlError::BadEndpoint(s.chars().take(200).collect()));
        }
        Ok(Self::Unix(PathBuf::from(s)))
    }

    /// Forme écrite dans le rendez-vous : chemin nu pour une socket Unix
    /// (J7), `tcp:127.0.0.1:<port>` pour TCP (C-CTL-11).
    #[must_use]
    pub fn to_rendezvous(&self) -> String {
        match self {
            Self::Unix(path) => path.to_string_lossy().into_owned(),
            Self::Tcp(_) => self.render(),
        }
    }

    /// Transport de l'adresse.
    #[must_use]
    pub const fn transport(&self) -> Transport {
        match self {
            Self::Unix(_) => Transport::Unix,
            Self::Tcp(_) => Transport::Tcp,
        }
    }

    /// Connexion à l'écouteur (client commun aux deux transports,
    /// C-CTL-11) ; `timeout` borne l'établissement d'une connexion TCP (une
    /// socket Unix répond ou refuse aussitôt).
    ///
    /// # Errors
    ///
    /// Celles du système ; [`io::ErrorKind::Unsupported`] pour une socket
    /// Unix hors Unix.
    pub fn connect(&self, timeout: Duration) -> io::Result<ControlStream> {
        match self {
            #[cfg(unix)]
            Self::Unix(path) => UnixStream::connect(path).map(|s| ControlStream(Inner::Unix(s))),
            #[cfg(not(unix))]
            Self::Unix(_) => Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "socket Unix indisponible sur cette plateforme",
            )),
            Self::Tcp(addr) => {
                let stream = TcpStream::connect_timeout(&SocketAddr::V4(*addr), non_zero(timeout))?;
                // Trames courtes, une réponse attendue : pas d'agrégation.
                let _ = stream.set_nodelay(true);
                Ok(ControlStream(Inner::Tcp(stream)))
            }
        }
    }
}

/// Délai jamais nul (`connect_timeout` et `set_read_timeout` refusent zéro).
fn non_zero(d: Duration) -> Duration {
    d.max(Duration::from_millis(1))
}

/// Flux d'une connexion du canal, socket Unix ou TCP.
#[derive(Debug)]
pub struct ControlStream(Inner);

#[derive(Debug)]
enum Inner {
    #[cfg(unix)]
    Unix(UnixStream),
    Tcp(TcpStream),
}

impl ControlStream {
    #[cfg(unix)]
    pub(super) const fn from_unix(s: UnixStream) -> Self {
        Self(Inner::Unix(s))
    }

    pub(super) const fn from_tcp(s: TcpStream) -> Self {
        Self(Inner::Tcp(s))
    }

    /// Délai de lecture (`None` : bloquant).
    ///
    /// # Errors
    ///
    /// Celles du système (délai nul refusé).
    pub fn set_read_timeout(&self, d: Option<Duration>) -> io::Result<()> {
        match &self.0 {
            #[cfg(unix)]
            Inner::Unix(s) => s.set_read_timeout(d),
            Inner::Tcp(s) => s.set_read_timeout(d),
        }
    }

    /// Délai d'écriture (`None` : bloquant).
    ///
    /// # Errors
    ///
    /// Celles du système (délai nul refusé).
    pub fn set_write_timeout(&self, d: Option<Duration>) -> io::Result<()> {
        match &self.0 {
            #[cfg(unix)]
            Inner::Unix(s) => s.set_write_timeout(d),
            Inner::Tcp(s) => s.set_write_timeout(d),
        }
    }

    /// Mode non bloquant.
    ///
    /// # Errors
    ///
    /// Celles du système.
    pub fn set_nonblocking(&self, on: bool) -> io::Result<()> {
        match &self.0 {
            #[cfg(unix)]
            Inner::Unix(s) => s.set_nonblocking(on),
            Inner::Tcp(s) => s.set_nonblocking(on),
        }
    }

    /// Second descripteur du même flux.
    ///
    /// # Errors
    ///
    /// Celles du système.
    pub fn try_clone(&self) -> io::Result<Self> {
        Ok(Self(match &self.0 {
            #[cfg(unix)]
            Inner::Unix(s) => Inner::Unix(s.try_clone()?),
            Inner::Tcp(s) => Inner::Tcp(s.try_clone()?),
        }))
    }

    /// Coupe le flux dans les deux sens (connexion évincée par
    /// l'écouteur) : la lecture en cours d'un autre fil se termine.
    pub(super) fn shutdown(&self) {
        let _ = match &self.0 {
            #[cfg(unix)]
            Inner::Unix(s) => s.shutdown(Shutdown::Both),
            Inner::Tcp(s) => s.shutdown(Shutdown::Both),
        };
    }

    /// Fin de connexion côté application. TCP : fin d'émission (`FIN`)
    /// puis lecture, 100 ms et 64 Kio au plus, de ce que le pair a déjà
    /// envoyé : fermer avec des octets non lus enverrait un `RST`, qui peut
    /// effacer chez le pair la dernière réponse (`control.busy`, refus du
    /// jeton) avant qu'il ne la lise. Socket Unix : rien à faire.
    #[cfg_attr(
        not(unix),
        allow(
            clippy::infallible_destructuring_match,
            reason = "hors Unix, TCP est le seul transport"
        )
    )]
    pub(super) fn close_gracefully(&self) {
        const DRAIN_BYTES: usize = 64 * 1024;
        let s = match &self.0 {
            #[cfg(unix)]
            Inner::Unix(_) => return,
            Inner::Tcp(s) => s,
        };
        if s.shutdown(Shutdown::Write).is_err()
            || s.set_read_timeout(Some(Duration::from_millis(100))).is_err()
        {
            return;
        }
        let mut buf = [0u8; 4096];
        let mut left = DRAIN_BYTES;
        while left > 0 {
            match (&*s).read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => left = left.saturating_sub(n),
            }
        }
    }
}

impl Read for &ControlStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match &self.0 {
            #[cfg(unix)]
            Inner::Unix(s) => (&*s).read(buf),
            Inner::Tcp(s) => (&*s).read(buf),
        }
    }
}

impl Write for &ControlStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match &self.0 {
            #[cfg(unix)]
            Inner::Unix(s) => (&*s).write(buf),
            Inner::Tcp(s) => (&*s).write(buf),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match &self.0 {
            #[cfg(unix)]
            Inner::Unix(s) => (&*s).flush(),
            Inner::Tcp(s) => (&*s).flush(),
        }
    }
}

impl Read for ControlStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        (&*self).read(buf)
    }
}

impl Write for ControlStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        (&*self).write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        (&*self).flush()
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::panic, reason = "tests")]

    use std::net::Ipv4Addr;

    use super::*;

    #[test]
    fn c_ctl_11_endpoint_roundtrip() {
        let tcp = Endpoint::parse("tcp:127.0.0.1:49152").unwrap();
        assert_eq!(tcp, Endpoint::Tcp(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 49152)));
        assert_eq!(tcp.render(), "tcp:127.0.0.1:49152");
        let unix = Endpoint::parse("unix:/run/user/1000/plannercraft/a.sock").unwrap();
        assert_eq!(unix.render(), "unix:/run/user/1000/plannercraft/a.sock");
        assert_eq!(Endpoint::parse(&unix.render()).unwrap(), unix);
    }

    #[test]
    fn c_ctl_11_endpoint_refuses_non_loopback_and_garbage() {
        for bad in [
            "",
            "tcp:",
            "tcp:0.0.0.0:4000",
            "tcp:192.168.1.16:4000",
            "tcp:127.0.0.1:0",
            "tcp:127.0.0.1",
            "tcp:[::1]:4000",
            "tcp:localhost:4000",
            "unix:",
            "pipe:plannercraft",
            "TCP:127.0.0.1:4000",
        ] {
            let err = Endpoint::parse(bad).unwrap_err();
            assert!(matches!(err, ControlError::BadEndpoint(_)), "{bad}");
        }
    }

    /// Forme du rendez-vous : chemin nu (J7) ou `tcp:` ; aller-retour.
    #[test]
    fn endpoint_parse_roundtrip() {
        let tcp = Endpoint::Tcp(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 50_001));
        assert_eq!(tcp.to_rendezvous(), "tcp:127.0.0.1:50001");
        assert_eq!(Endpoint::from_rendezvous(&tcp.to_rendezvous()).unwrap(), tcp);
        assert_eq!(Endpoint::parse(&tcp.render()).unwrap(), tcp);
        assert_eq!(tcp.transport(), Transport::Tcp);
        let other_loopback = Endpoint::parse("tcp:127.0.0.2:7").unwrap();
        assert_eq!(Endpoint::parse(&other_loopback.render()).unwrap(), other_loopback);

        let unix = Endpoint::Unix(PathBuf::from("/run/user/1000/plannercraft/1-0.sock"));
        assert_eq!(unix.to_rendezvous(), "/run/user/1000/plannercraft/1-0.sock");
        assert_eq!(unix.transport(), Transport::Unix);
        if cfg!(unix) {
            // Chemin nu du J7 : relu comme socket Unix.
            assert_eq!(Endpoint::from_rendezvous(&unix.to_rendezvous()).unwrap(), unix);
        }
        assert_eq!(Endpoint::from_rendezvous(&unix.render()).unwrap(), unix);
        assert_eq!(Transport::NATIVE == Transport::Unix, cfg!(unix));
    }

    #[test]
    fn endpoint_bad_refused() {
        for bad in [
            "",
            "relatif.sock",
            "tcp:10.0.0.1:80",
            "tcp:127.0.0.1:0",
            "tcp:127.0.0.1:99999",
            "unix:",
            "/a\0b",
            "http://127.0.0.1:80",
        ] {
            let err = Endpoint::from_rendezvous(bad).unwrap_err();
            assert!(matches!(err, ControlError::BadEndpoint(_)), "{bad}");
        }
        // Message borné : une adresse énorme n'est pas recopiée.
        let long = format!("tcp:{}", "9".repeat(10_000));
        let ControlError::BadEndpoint(shown) = Endpoint::parse(&long).unwrap_err() else {
            panic!("bad_endpoint attendu");
        };
        assert_eq!(shown.chars().count(), 200);
    }
}
