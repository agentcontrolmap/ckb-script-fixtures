#![no_std]
#![no_main]

use ckb_std::{
    ckb_constants::{CellField, InputField, Source},
    error::SysError,
    syscalls,
};

ckb_std::entry!(program_entry);
ckb_std::default_alloc!();

fn program_entry() -> i8 {
    match run() {
        Ok(()) => 0,
        Err(c) => c,
    }
}

fn e(_: SysError) -> i8 {
    100
}

fn cap(index: usize, source: Source) -> Result<u64, SysError> {
    let mut b = [0u8; 8];
    syscalls::load_cell_by_field(&mut b, 0, index, source, CellField::Capacity)?;
    Ok(u64::from_le_bytes(b))
}

fn lock_hash(index: usize, source: Source) -> Result<[u8; 32], SysError> {
    let mut b = [0u8; 32];
    syscalls::load_cell_by_field(&mut b, 0, index, source, CellField::LockHash)?;
    Ok(b)
}

fn since(index: usize, source: Source) -> Result<u64, SysError> {
    let mut b = [0u8; 8];
    syscalls::load_input_by_field(&mut b, 0, index, source, InputField::Since)?;
    Ok(u64::from_le_bytes(b))
}

fn state(index: usize, source: Source) -> Result<u8, SysError> {
    let mut b = [0u8; 1];
    let n = syscalls::load_cell_data(&mut b, 0, index, source)?;
    if n == 0 {
        return Ok(0);
    }
    Ok(b[0])
}

// group inputs: (total capacity, every cell has state byte == 1)
fn group_inputs() -> Result<(u64, bool), i8> {
    let mut total = 0u64;
    let mut all = true;
    let mut i = 0;
    loop {
        match cap(i, Source::GroupInput) {
            Ok(c) => total += c,
            Err(SysError::IndexOutOfBound) => break,
            Err(_) => return Err(20),
        }
        if state(i, Source::GroupInput).map_err(e)? != 1 {
            all = false;
        }
        i += 1;
    }
    Ok((total, all))
}

// capacity of all outputs whose lock hash equals h
fn paid_to(h: &[u8; 32]) -> Result<u64, i8> {
    let mut total = 0u64;
    let mut i = 0;
    loop {
        match lock_hash(i, Source::Output) {
            Ok(lh) => {
                if &lh == h {
                    total += cap(i, Source::Output).map_err(e)?;
                }
            }
            Err(SysError::IndexOutOfBound) => break,
            Err(_) => return Err(21),
        }
        i += 1;
    }
    Ok(total)
}

fn run() -> Result<(), i8> {
    // Script is a molecule table: size | 3 offsets | code_hash(32) | hash_type(1) | args(len + bytes)
    let mut sb = [0u8; 256];
    let n = syscalls::load_script(&mut sb, 0).map_err(e)?;
    if n > 256 || n < 53 {
        return Err(3);
    }
    let off2 = u32::from_le_bytes([sb[12], sb[13], sb[14], sb[15]]) as usize;
    if off2 + 4 > 256 {
        return Err(3);
    }
    let alen = u32::from_le_bytes([sb[off2], sb[off2 + 1], sb[off2 + 2], sb[off2 + 3]]) as usize;
    if alen != 73 || off2 + 4 + 73 > 256 {
        return Err(2);
    }
    let args = &sb[off2 + 4..off2 + 4 + 73];
    let mode = args[0];
    if mode == 9 {
        return Ok(()); // always success, used only as a plain funding lock in tests
    }
    let mut recipient = [0u8; 32];
    recipient.copy_from_slice(&args[1..33]);
    let mut refund = [0u8; 32];
    refund.copy_from_slice(&args[33..65]);
    let mut dl = [0u8; 8];
    dl.copy_from_slice(&args[65..73]);
    let deadline = u64::from_le_bytes(dl);

    let mut wb = [0u8; 16];
    let wn = syscalls::load_witness(&mut wb, 0, 0, Source::GroupInput).map_err(e)?;
    if wn == 0 {
        return Err(4);
    }
    let path = wb[0];

    match (path, mode) {
        // release, naive: looks only at the first cell of the group and the first output
        (0, 0) => {
            if state(0, Source::GroupInput).map_err(e)? != 1 {
                return Err(10);
            }
            let cin = cap(0, Source::GroupInput).map_err(e)?;
            let lh = lock_hash(0, Source::Output).map_err(e)?;
            if lh != recipient {
                return Err(11);
            }
            let cout = cap(0, Source::Output).map_err(e)?;
            if cout < cin {
                return Err(12);
            }
            Ok(())
        }
        // release, hardened: every cell of the group approved, all capacity goes to the recipient
        (0, 1) => {
            let (total, all) = group_inputs()?;
            if !all {
                return Err(13);
            }
            if paid_to(&recipient)? < total {
                return Err(15);
            }
            Ok(())
        }
        // refund, naive: only checks that the first output goes to the refund address
        (1, 0) => {
            let lh = lock_hash(0, Source::Output).map_err(e)?;
            if lh != refund {
                return Err(14);
            }
            Ok(())
        }
        // refund, hardened: absolute block-number since >= deadline on every cell, no capacity lost
        (1, 1) => {
            let mut i = 0;
            loop {
                match since(i, Source::GroupInput) {
                    Ok(s) => {
                        if (s >> 61) != 0 {
                            return Err(30); // relative flag or wrong metric
                        }
                        if (s & 0x00FF_FFFF_FFFF_FFFF) < deadline {
                            return Err(31); // too early
                        }
                    }
                    Err(SysError::IndexOutOfBound) => break,
                    Err(_) => return Err(22),
                }
                i += 1;
            }
            let (total, _) = group_inputs()?;
            if paid_to(&refund)? < total {
                return Err(32);
            }
            Ok(())
        }
        _ => Err(5),
    }
}
