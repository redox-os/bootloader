pub unsafe fn paging() {
    // Disable MMU
    asm!(
        "mrs     x0, sctlr_el1",
        "bic     x0, x0, 1",
        "msr     sctlr_el1, x0",
        "isb",
        lateout("x0") _
    );
}
