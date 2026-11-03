//! Transport TCP en boucle locale du canal de contrôle (C-CTL-11 à
//! C-CTL-14, ADR-0016 D12, lot L8) : écouteur lié à `127.0.0.1:0` (port
//! choisi par le système), pair hors boucle locale fermé sans lecture,
//! première ligne invalide ⇒ connexion fermée sans réponse (dans
//! l'écouteur, commun aux deux transports).
//!
//! Transport de Windows (pas de socket Unix dans std) ; compilé et testé sur
//! tous les systèmes (`Transport::Tcp`). Sans `unsafe` ni dépendance : std
//! seul. La boucle locale n'est pas privée à l'utilisateur comme un dossier
//! 0700 : un autre compte de la machine peut se connecter, le jeton (32
//! octets, rendez-vous 0600 ou `%LOCALAPPDATA%`) reste la seule clé.

use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, TcpListener};

use super::ControlError;

/// Écouteur lié à `127.0.0.1:0` (port choisi par le système, C-CTL-11).
///
/// # Errors
///
/// [`ControlError::Io`] si le système refuse la liaison.
pub fn bind_loopback() -> Result<TcpListener, ControlError> {
    TcpListener::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0))
        .map_err(|e| ControlError::io("ouverture du port en boucle locale", &e))
}

/// Adresse réelle de l'écouteur (`127.0.0.1:<port>`).
///
/// # Errors
///
/// [`ControlError::Io`] ; [`ControlError::BadEndpoint`] si l'écouteur n'est
/// pas lié en IPv4 sur la boucle locale.
pub fn local_endpoint(listener: &TcpListener) -> Result<SocketAddrV4, ControlError> {
    match listener
        .local_addr()
        .map_err(|e| ControlError::io("adresse de l'écouteur", &e))?
    {
        SocketAddr::V4(addr) if addr.ip().is_loopback() && addr.port() != 0 => Ok(addr),
        other => Err(ControlError::BadEndpoint(other.to_string())),
    }
}

/// Vrai si le pair peut être lu (C-CTL-12 : boucle locale seulement ; une
/// adresse IPv6 qui porte une IPv4 est ramenée à celle-ci). Un pair refusé
/// est fermé sans qu'un octet soit lu.
#[must_use]
pub fn accept_peer(peer: &SocketAddr) -> bool {
    peer.ip().to_canonical().is_loopback()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, reason = "tests")]

    use std::net::{IpAddr, Ipv6Addr};

    use super::*;

    #[test]
    fn c_ctl_11_bind_loopback_only() {
        let listener = bind_loopback().unwrap();
        let addr = local_endpoint(&listener).unwrap();
        assert_eq!(*addr.ip(), Ipv4Addr::LOCALHOST);
        assert_ne!(addr.port(), 0);
        // Deux écouteurs : deux ports.
        let second = bind_loopback().unwrap();
        assert_ne!(local_endpoint(&second).unwrap().port(), addr.port());
        // Un écouteur lié ailleurs n'est pas une adresse de rendez-vous.
        let any = TcpListener::bind("0.0.0.0:0").unwrap();
        assert!(matches!(local_endpoint(&any), Err(ControlError::BadEndpoint(_))));
    }

    #[test]
    fn c_ctl_12_non_loopback_peer_refused() {
        let port = 50_000;
        let peer = |ip: IpAddr| SocketAddr::new(ip, port);
        for ok in [
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            IpAddr::V4(Ipv4Addr::new(127, 0, 0, 2)),
            IpAddr::V4(Ipv4Addr::new(127, 255, 255, 254)),
            IpAddr::V6(Ipv6Addr::LOCALHOST),
            IpAddr::V6(Ipv4Addr::LOCALHOST.to_ipv6_mapped()),
        ] {
            assert!(accept_peer(&peer(ok)), "{ok}");
        }
        for refused in [
            IpAddr::V4(Ipv4Addr::UNSPECIFIED),
            IpAddr::V4(Ipv4Addr::new(192, 168, 1, 16)),
            IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1)),
            IpAddr::V4(Ipv4Addr::new(100, 81, 95, 91)),
            IpAddr::V4(Ipv4Addr::new(128, 0, 0, 1)),
            IpAddr::V4(Ipv4Addr::BROADCAST),
            IpAddr::V6(Ipv6Addr::UNSPECIFIED),
            IpAddr::V6(Ipv4Addr::new(192, 168, 1, 16).to_ipv6_mapped()),
            IpAddr::V6(Ipv6Addr::new(0xfe80, 0, 0, 0, 0, 0, 0, 1)),
        ] {
            assert!(!accept_peer(&peer(refused)), "{refused}");
        }
    }
}
