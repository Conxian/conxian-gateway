# Qualify native persistent storage before a decentralised placement

Refs #466, #547; Conxian/conxian-business#1390 and #1423.

Preserve the current exclusive-local-writer backend and its file/parent fsync,
atomic replacement, revision CAS and process ownership contracts. Compare
replicated block devices under a single ext4/XFS mount with decentralised object
backup. Do not mount an object gateway as the authoritative state directory or
present a pending SDK integration as a working database.

Add an operator command that runs the existing native persistence regression
suite on an explicitly selected storage directory. It must not reimplement CAS,
read existing Gateway state, change startup/CI/release policies, or declare storage
production-qualified from component tests. Empty/skipped suites and lost mandatory
test cases must fail; raw output must not be parsed as a fabricated success.

Acceptance: targeted revision conflict, ownership exclusion/release, crash/restart,
corrupt-state, atomic replacement and uncertain-commit tests run on that selected
filesystem. Physical power loss, distributed fencing/quorum, recovery and operator
independence remain separately measured release requirements. Compare Sia's actual
current object interface and local metadata recovery instead of relying only on
an older roadmap. Keep confidential findings in their existing protected channel.
