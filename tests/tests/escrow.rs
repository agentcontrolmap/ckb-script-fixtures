use ckb_testtool::{
    ckb_types::{bytes::Bytes, core::TransactionBuilder, packed::*, prelude::*},
    context::Context,
};

const MAX_CYCLES: u64 = 10_000_000;
const DEADLINE: u64 = 1000;

fn env() -> (Context, OutPoint, CellDep) {
    let path = std::env::var("ESCROW_BIN").expect("ESCROW_BIN not set");
    let bin = std::fs::read(path).expect("cannot read escrow binary");
    let mut ctx = Context::default();
    let op = ctx.deploy_cell(Bytes::from(bin));
    let dep = CellDep::new_builder().out_point(op.clone()).build();
    (ctx, op, dep)
}

fn dummy_lock(tag: u8) -> Script {
    Script::new_builder()
        .args(Bytes::from(vec![tag]).pack())
        .build()
}

fn lock_args(mode: u8, recipient: &Script, refund: &Script, deadline: u64) -> Bytes {
    let mut v = vec![mode];
    v.extend_from_slice(recipient.calc_script_hash().as_slice());
    v.extend_from_slice(refund.calc_script_hash().as_slice());
    v.extend_from_slice(&deadline.to_le_bytes());
    Bytes::from(v)
}

fn cell(ctx: &mut Context, lock: &Script, cap: u64, state: u8) -> CellInput {
    let op = ctx.create_cell(
        CellOutput::new_builder()
            .capacity(cap)
            .lock(lock.clone())
            .build(),
        Bytes::from(vec![state]),
    );
    CellInput::new_builder().previous_output(op).build()
}

fn with_since(i: CellInput, s: u64) -> CellInput {
    i.as_builder().since(s).build()
}

fn out(lock: &Script, cap: u64) -> CellOutput {
    CellOutput::new_builder()
        .capacity(cap)
        .lock(lock.clone())
        .build()
}

fn spend(
    ctx: &mut Context,
    dep: &CellDep,
    inputs: Vec<CellInput>,
    outputs: Vec<CellOutput>,
    datas: Vec<Bytes>,
    path: u8,
) -> Result<(), String> {
    let witnesses: Vec<_> = inputs
        .iter()
        .map(|_| Bytes::from(vec![path]).pack())
        .collect();
    let tx = TransactionBuilder::default()
        .cell_dep(dep.clone())
        .inputs(inputs)
        .outputs(outputs)
        .outputs_data(datas.into_iter().map(|d| d.pack()).collect::<Vec<_>>())
        .witnesses(witnesses)
        .build();
    let tx = ctx.complete_tx(tx);
    ctx.verify_tx(&tx, MAX_CYCLES)
        .map(|_| ())
        .map_err(|e| format!("{:?}", e))
}

fn empty(n: usize) -> Vec<Bytes> {
    (0..n).map(|_| Bytes::new()).collect()
}

fn report(name: &str, mode: u8, expect_ok: bool, r: &Result<(), String>) {
    let label = if mode == 0 { "naive" } else { "hardened" };
    let got = match r {
        Ok(()) => "ACCEPTED".to_string(),
        Err(e) => format!("REJECTED ({})", e.chars().take(700).collect::<String>()),
    };
    println!("RESULT | {} | {} | {}", name, label, got);
    assert_eq!(r.is_ok(), expect_ok, "{} {}", name, label);
}

// A. one approved cell + one NOT approved cell with the same lock args in one transaction
#[test]
fn a_group_release_second_cell_not_approved() {
    for mode in [0u8, 1u8] {
        let (mut ctx, op, dep) = env();
        let recip = dummy_lock(1);
        let refund = dummy_lock(2);
        let lock = ctx
            .build_script(&op, lock_args(mode, &recip, &refund, DEADLINE))
            .unwrap();
        let a = cell(&mut ctx, &lock, 1000, 1);
        let b = cell(&mut ctx, &lock, 1000, 0);
        let r = spend(
            &mut ctx,
            &dep,
            vec![a, b],
            vec![out(&recip, 1000), out(&recip, 1000)],
            empty(2),
            0,
        );
        report("A release: 2nd cell of the group NOT approved", mode, mode == 0, &r);
    }
}

// A-control. one approved cell, normal release
#[test]
fn a_control_single_approved_release() {
    for mode in [0u8, 1u8] {
        let (mut ctx, op, dep) = env();
        let recip = dummy_lock(1);
        let refund = dummy_lock(2);
        let lock = ctx
            .build_script(&op, lock_args(mode, &recip, &refund, DEADLINE))
            .unwrap();
        let a = cell(&mut ctx, &lock, 1000, 1);
        let r = spend(&mut ctx, &dep, vec![a], vec![out(&recip, 1000)], empty(1), 0);
        report("control: single approved cell released", mode, true, &r);
    }
}

// B. refund with different `since` values
#[test]
fn b_refund_since() {
    let cases: Vec<(&str, u64, bool)> = vec![
        ("since = 0 (no precondition)", 0, false),
        ("absolute block number 999 (before deadline)", 999, false),
        ("absolute block number 1000 (at deadline)", 1000, true),
        ("relative flag set, value 1000", (1u64 << 63) | 1000, false),
        ("timestamp metric, value 1000", (2u64 << 61) | 1000, false),
    ];
    for (name, since, ok_hardened) in cases {
        for mode in [0u8, 1u8] {
            let (mut ctx, op, dep) = env();
            let recip = dummy_lock(1);
            let refund = dummy_lock(2);
            let lock = ctx
                .build_script(&op, lock_args(mode, &recip, &refund, DEADLINE))
                .unwrap();
            let a = with_since(cell(&mut ctx, &lock, 1000, 0), since);
            let r = spend(&mut ctx, &dep, vec![a], vec![out(&refund, 1000)], empty(1), 1);
            let expect = if mode == 0 { true } else { ok_hardened };
            report(&format!("B refund, {}", name), mode, expect, &r);
        }
    }
}

// C. refund whose outputs are smaller than the input (difference would go to the miner as fee)
#[test]
fn c_refund_outputs_below_input() {
    for mode in [0u8, 1u8] {
        let (mut ctx, op, dep) = env();
        let recip = dummy_lock(1);
        let refund = dummy_lock(2);
        let lock = ctx
            .build_script(&op, lock_args(mode, &recip, &refund, DEADLINE))
            .unwrap();
        let a = with_since(cell(&mut ctx, &lock, 1000, 0), DEADLINE);
        let r = spend(&mut ctx, &dep, vec![a], vec![out(&refund, 400)], empty(1), 1);
        report("C refund pays 400 of 1000 (600 would be fee)", mode, mode == 0, &r);
    }
}

// D. creation: anyone can create a cell with our lock, any args, any data; no escrow script runs
#[test]
fn d_forged_creation_is_not_checked() {
    let (mut ctx, op, dep) = env();
    let recip = dummy_lock(1);
    let refund = dummy_lock(2);
    // plain funding lock (mode 9 = always success)
    let funding = ctx
        .build_script(&op, lock_args(9, &recip, &refund, 0))
        .unwrap();
    // escrow lock with deadline 0 and state already "approved"
    let forged = ctx
        .build_script(&op, lock_args(1, &recip, &refund, 0))
        .unwrap();
    let a = cell(&mut ctx, &funding, 1000, 0);
    let r = spend(
        &mut ctx,
        &dep,
        vec![a],
        vec![out(&forged, 1000)],
        vec![Bytes::from(vec![1u8])],
        0,
    );
    report("D new escrow cell with deadline 0 and state approved", 1, true, &r);
}
