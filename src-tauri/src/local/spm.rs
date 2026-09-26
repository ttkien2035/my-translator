//! Minimal reader for a SentencePiece `bpe.model` (protobuf `ModelProto`).
//! Only `pieces` (field 1; inside: `piece` string = field 1, `score` float =
//! field 2) is read, to write the `bpe.vocab` that sherpa-onnx's hotword
//! encoder needs and that the X-ASR archive does not ship.

pub struct Piece {
    pub piece: String,
    pub score: f32,
}

fn varint(b: &[u8], i: &mut usize) -> Option<u64> {
    let (mut v, mut shift) = (0u64, 0u32);
    loop {
        let byte = *b.get(*i)?;
        *i += 1;
        v |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return Some(v);
        }
        shift += 7;
        if shift > 63 {
            return None;
        }
    }
}

/// Skip one field of the given wire type.
fn skip(b: &[u8], i: &mut usize, wire: u64) -> Option<()> {
    match wire {
        0 => {
            varint(b, i)?;
        }
        1 => *i = i.checked_add(8)?,
        2 => {
            let n = usize::try_from(varint(b, i)?).ok()?;
            *i = i.checked_add(n)?;
        }
        5 => *i = i.checked_add(4)?,
        _ => return None,
    }
    (*i <= b.len()).then_some(())
}

/// Length-delimited payload starting at `*i`; advances past it.
fn bytes<'a>(b: &'a [u8], i: &mut usize) -> Option<&'a [u8]> {
    let n = usize::try_from(varint(b, i)?).ok()?;
    let end = i.checked_add(n).filter(|e| *e <= b.len())?;
    let out = &b[*i..end];
    *i = end;
    Some(out)
}

/// All pieces of a `bpe.model`, in id order.
pub fn pieces(model: &[u8]) -> Result<Vec<Piece>, String> {
    let mut out = Vec::with_capacity(8192);
    let mut i = 0;
    while i < model.len() {
        let key = varint(model, &mut i).ok_or("bpe.model: truncated")?;
        match (key >> 3, key & 7) {
            (1, 2) => out.push(piece(bytes(model, &mut i).ok_or("bpe.model: truncated piece")?)?),
            (_, wire) => skip(model, &mut i, wire).ok_or("bpe.model: bad field")?,
        }
    }
    Ok(out)
}

fn piece(b: &[u8]) -> Result<Piece, String> {
    let (mut piece, mut score, mut i) = (String::new(), 0f32, 0usize);
    while i < b.len() {
        let key = varint(b, &mut i).ok_or("bpe.model: truncated")?;
        match (key >> 3, key & 7) {
            (1, 2) => piece = String::from_utf8_lossy(bytes(b, &mut i).ok_or("bpe.model: truncated")?).into_owned(),
            (2, 5) => {
                let end = i.checked_add(4).filter(|e| *e <= b.len()).ok_or("bpe.model: truncated")?;
                score = f32::from_le_bytes(b[i..end].try_into().expect("4 bytes"));
                i = end;
            }
            (_, wire) => skip(b, &mut i, wire).ok_or("bpe.model: bad field")?,
        }
    }
    Ok(Piece { piece, score })
}

/// `bpe.vocab` text as ssentencepiece (inside sherpa-onnx) reads it:
/// one `piece<TAB>score` per line, in id order.
pub fn vocab_text(pieces: &[Piece]) -> String {
    use std::fmt::Write as _;
    let mut s = String::with_capacity(pieces.len() * 16);
    for p in pieces {
        let _ = writeln!(s, "{}\t{}", p.piece, p.score);
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Encode one `SentencePiece` message inside `ModelProto.pieces`.
    fn enc(piece: &str, score: f32, ty: u8) -> Vec<u8> {
        let mut inner = vec![0x0a, piece.len() as u8];
        inner.extend(piece.as_bytes());
        inner.push(0x15);
        inner.extend(score.to_le_bytes());
        inner.extend([0x18, ty]);
        let mut out = vec![0x0a, inner.len() as u8];
        out.extend(inner);
        out
    }

    #[test]
    fn parses_pieces_and_skips_other_fields() {
        // First bytes of X-ASR's real bpe.model: piece "<blk>", score 0, type 4.
        let real = [0x0a, 0x0e, 0x0a, 0x05, b'<', b'b', b'l', b'k', b'>', 0x15, 0, 0, 0, 0, 0x18, 0x04];
        let mut model = real.to_vec();
        model.extend(enc("▁协", -8.5, 1));
        // A TrainerSpec (field 2, length-delimited) that must be skipped.
        model.extend([0x12, 0x03, 0x08, 0x01, 0x00]);
        model.extend(enc("J", -14.475_982, 1));
        let p = pieces(&model).unwrap();
        assert_eq!(p.len(), 3);
        assert_eq!((p[0].piece.as_str(), p[0].score), ("<blk>", 0.0));
        assert_eq!((p[1].piece.as_str(), p[1].score), ("▁协", -8.5));
        assert_eq!(p[2].piece, "J");
        assert_eq!(vocab_text(&p[..2]), "<blk>\t0\n▁协\t-8.5\n");
        assert!(pieces(&model[..model.len() - 3]).is_err(), "truncated file is an error, not a panic");
    }

    /// The real model (`MT_TEST_XASR_DIR/bpe.model`): 5 000 pieces, ids match sentencepiece.
    #[test]
    #[ignore]
    fn reads_real_bpe_model() {
        let Ok(dir) = std::env::var("MT_TEST_XASR_DIR") else { return };
        let p = pieces(&std::fs::read(std::path::Path::new(&dir).join("bpe.model")).unwrap()).unwrap();
        assert_eq!(p.len(), 5000);
        assert_eq!(p[0].piece, "<blk>");
        assert_eq!(p[99].piece, "▁开");
        assert_eq!(p[4999].piece, "J");
        assert!((p[4999].score + 14.475_982).abs() < 1e-4);
    }
}
