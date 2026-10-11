# Worker replacement Save at exit

production_worker_shutdown_persists_replacement_save passes12 cases: Shutdown,
command-channel disconnect and guest File > Quit across mono68k, colour68k,
PPC8 and PPC16 (82.86s). The setup persists distinct old data and a nonempty
resource fork. The actual production worker imports it, commits the filename,
accepts Save and confirms replacement through existing guest event paths.
After the panel returns, each exit route joins the worker. The test does not
explicitly flush the replacement worker. Persisted new data and unchanged
resource fork are checked, then a separate reader OS process performs guest
Open/GetEOF/FSRead/FSClose and compares exact forks and metadata.

This establishes final persisted contents after orderly exits, not whether
periodic or final flush wrote them, physical window close, interruption recovery
or crash durability. The sequential store-write source audit remains open.
