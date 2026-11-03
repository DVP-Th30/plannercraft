//! Lecteur borné : refuse une ligne d'entrée plus longue que la borne
//! (`io::ErrorKind::InvalidData`), le transport rmcp ferme alors proprement
//! (C-MCP-06, lot L8).
//!
//! Le transport de rmcp lit stdin ligne par ligne sans borne
//! (`async_rw.rs`) : sans ce lecteur, une ligne de plusieurs gigaoctets
//! serait accumulée en mémoire. Les octets des lignes complètes qui
//! précèdent la ligne fautive sont rendus avant l'erreur, pour que les
//! requêtes déjà reçues soient servies.

use std::io;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::task::{Context, Poll};

use tokio::io::{AsyncRead, ReadBuf};

/// Lecteur qui compte les octets depuis le dernier `\n`.
#[derive(Debug)]
pub(crate) struct BoundedLines<R> {
    inner: R,
    max_line_bytes: usize,
    current: usize,
    /// Une ligne a dépassé la borne : l'erreur est rendue au prochain appel
    /// (après les lignes complètes de la même lecture).
    pending_error: bool,
    /// Signal partagé avec le serveur : la session s'est close sur une
    /// ligne trop longue.
    tripped: Arc<AtomicBool>,
}

impl<R> BoundedLines<R> {
    /// Enveloppe `inner` ; `tripped` passe à vrai quand une ligne dépasse
    /// `max_line_bytes` octets (fin de ligne exclue).
    pub(crate) const fn new(inner: R, max_line_bytes: usize, tripped: Arc<AtomicBool>) -> Self {
        Self {
            inner,
            max_line_bytes,
            current: 0,
            pending_error: false,
            tripped,
        }
    }

    fn error(&self) -> io::Error {
        self.tripped.store(true, Ordering::SeqCst);
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("ligne d'entrée de plus de {} octets", self.max_line_bytes),
        )
    }

    /// Compte les octets lus ; `Some(n)` si la borne saute : `n` octets
    /// (jusqu'au dernier `\n` avant le dépassement) restent valides.
    fn scan(&mut self, bytes: &[u8]) -> Option<usize> {
        let mut last_newline_end = 0usize;
        for (i, &b) in bytes.iter().enumerate() {
            if b == b'\n' {
                self.current = 0;
                last_newline_end = i.saturating_add(1);
            } else {
                self.current = self.current.saturating_add(1);
                if self.current > self.max_line_bytes {
                    return Some(last_newline_end);
                }
            }
        }
        None
    }
}

impl<R: AsyncRead + Unpin> AsyncRead for BoundedLines<R> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        if self.pending_error {
            return Poll::Ready(Err(self.error()));
        }
        let before = buf.filled().len();
        match Pin::new(&mut self.inner).poll_read(cx, buf) {
            Poll::Ready(Ok(())) => {}
            other => return other,
        }
        let fresh = buf.filled().get(before..).unwrap_or_default();
        let Some(keep) = self.scan(fresh) else {
            return Poll::Ready(Ok(()));
        };
        if keep == 0 {
            // Contrat d'`AsyncRead` : une erreur ne rend aucun octet.
            buf.set_filled(before);
            return Poll::Ready(Err(self.error()));
        }
        // Rend les lignes complètes ; l'erreur suit au prochain appel.
        buf.set_filled(before.saturating_add(keep));
        self.pending_error = true;
        Poll::Ready(Ok(()))
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::assert_is_empty,
        reason = "tests : un échec doit s'arrêter net"
    )]

    use tokio::io::AsyncReadExt;

    use super::*;

    async fn read_all(input: &[u8], max: usize) -> (Vec<u8>, io::Result<usize>, bool) {
        let tripped = Arc::new(AtomicBool::new(false));
        let mut r = BoundedLines::new(input, max, Arc::clone(&tripped));
        let mut out = Vec::new();
        let res = r.read_to_end(&mut out).await;
        (out, res, tripped.load(Ordering::SeqCst))
    }

    #[tokio::test]
    async fn bounded_line_at_limit_passes() {
        let input = b"abcd\nefgh\n";
        let (out, res, tripped) = read_all(input, 4).await;
        assert_eq!(res.unwrap(), input.len());
        assert_eq!(out, input);
        assert!(!tripped);
    }

    #[tokio::test]
    async fn bounded_line_over_limit_refused_after_complete_lines() {
        let (out, res, tripped) = read_all(b"ab\ncdefg\nhi\n", 4).await;
        assert_eq!(res.unwrap_err().kind(), io::ErrorKind::InvalidData);
        // La ligne complète qui précède est rendue avant l'erreur.
        assert_eq!(out, b"ab\n");
        assert!(tripped);
    }

    #[tokio::test]
    async fn bounded_first_line_over_limit_refused() {
        let (out, res, tripped) = read_all(b"abcdefgh", 4).await;
        assert_eq!(res.unwrap_err().kind(), io::ErrorKind::InvalidData);
        assert!(out.is_empty());
        assert!(tripped);
    }
}
