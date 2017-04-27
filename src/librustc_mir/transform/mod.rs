// Copyright 2015 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use rustc::hir::def_id::DefId;
use rustc::mir::Mir;
use rustc::mir::transform::{MirCtxt, MirPassIndex, MirPassSet, MirSource, MIR_OPTIMIZED};
use rustc::ty::TyCtxt;
use rustc::ty::maps::Providers;
use std::cell::{Ref, RefCell};
use std::mem;

pub mod simplify_branches;
pub mod simplify;
pub mod erase_regions;
pub mod no_landing_pads;
pub mod type_check;
pub mod add_call_guards;
pub mod promote_consts;
pub mod qualify_consts;
pub mod dump_mir;
pub mod deaggregator;
pub mod instcombine;
pub mod copy_prop;
pub mod inline;

pub fn provide(providers: &mut Providers) {
    self::qualify_consts::provide(providers);
    *providers = Providers {
        optimized_mir,
        mir_pass_set,
        mir_pass,
        ..*providers
    };
}

fn optimized_mir<'a, 'tcx>(tcx: TyCtxt<'a, 'tcx, 'tcx>, def_id: DefId) -> &'tcx RefCell<Mir<'tcx>> {
    let mir = tcx.mir_pass_set((MIR_OPTIMIZED, def_id));

    // "lock" the ref cell into read mode; after this point,
    // there ought to be no more changes to the MIR.
    mem::drop(mir.borrow());

    mir
}

fn mir_pass_set<'a, 'tcx>(tcx: TyCtxt<'a, 'tcx, 'tcx>,
                          (pass_set, def_id): (MirPassSet, DefId))
                          -> &'tcx RefCell<Mir<'tcx>>
{
    let passes = &tcx.mir_passes;
    let len = passes.len_passes(pass_set);
    assert!(len > 0, "no passes in {:?}", pass_set);
    tcx.mir_pass((pass_set, MirPassIndex(len - 1), def_id))
}

fn mir_pass<'a, 'tcx>(tcx: TyCtxt<'a, 'tcx, 'tcx>,
                      (pass_set, pass_num, def_id): (MirPassSet, MirPassIndex, DefId))
                      -> &'tcx RefCell<Mir<'tcx>>
{
    let passes = &tcx.mir_passes;
    let pass = passes.pass(pass_set, pass_num);
    let mir_ctxt = MirCtxtImpl { tcx, pass_num, pass_set, def_id };

    for hook in passes.hooks() {
        hook.on_mir_pass(&mir_ctxt, None);
    }

    let mir = pass.run_pass(&mir_ctxt);

    for hook in passes.hooks() {
        hook.on_mir_pass(&mir_ctxt, Some(&mir.borrow()));
    }

    mir
}

struct MirCtxtImpl<'a, 'tcx: 'a> {
    tcx: TyCtxt<'a, 'tcx, 'tcx>,
    pass_num: MirPassIndex,
    pass_set: MirPassSet,
    def_id: DefId
}

impl<'a, 'tcx> MirCtxt<'a, 'tcx> for MirCtxtImpl<'a, 'tcx> {
    fn tcx(&self) -> TyCtxt<'a, 'tcx, 'tcx> {
        self.tcx
    }

    fn pass_set(&self) -> MirPassSet {
        self.pass_set
    }

    fn pass_num(&self) -> MirPassIndex {
        self.pass_num
    }

    fn def_id(&self) -> DefId {
        self.def_id
    }

    fn source(&self) -> MirSource {
        let id = self.tcx.hir.as_local_node_id(self.def_id)
                             .expect("mir source requires local def-id");
        MirSource::from_node(self.tcx, id)
    }

    fn read_previous_mir(&self) -> Ref<'tcx, Mir<'tcx>> {
        self.steal_previous_mir().borrow()
    }

    fn steal_previous_mir(&self) -> &'tcx RefCell<Mir<'tcx>> {
        let MirPassSet(pass_set) = self.pass_set;
        let MirPassIndex(pass_num) = self.pass_num;
        if pass_num > 0 {
            self.tcx.mir_pass((MirPassSet(pass_set), MirPassIndex(pass_num - 1), self.def_id))
        } else if pass_set > 0 {
            self.tcx.mir_pass_set((MirPassSet(pass_set - 1), self.def_id))
        } else {
            self.tcx.mir_build(self.def_id)
        }
    }
}
