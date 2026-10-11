# Replacement Save across independent processes

Run the example test standard_file_save_survives_separate_process_and_guest_read
with features gpui-demo-test and macOS services available. Each CPU/depth case
runs three distinct OS processes: initial new-file Save, guest-confirmed
replacement Save, and guest Open/readback. The replacement process first imports
and explicitly persists distinct old data plus a seven-byte binary resource fork,
then reloads the store and verifies both. Real guest Save acceptance requires
replacement confirmation. Guest FSpOpenDF/SetEOF/FSWrite/FSClose updates the data
fork; the unrelated resource fork must remain exact.

The read process imports from the production desktop store, initializes the
application, selects the restored Open row and checks the guest memory published
after GetEOF/FSRead/FSClose. Exact data/resource forks and metadata must match
before and after guest read. All twelve phases pass across mono68k, colour68k,
PPC8 and PPC16. The initial data-only run passes24.05s; the stronger resource-fork
run passes20.99s. Captures are not required for the byte durability assertion.

This qualifies explicit sync_save_files_now, not production-worker periodic or
shutdown flush, physical desktop closing or crash durability. No release gate
closes from this selected regression alone.
