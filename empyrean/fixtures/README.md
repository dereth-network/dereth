# Server test data

`vectors/` holds ACE's answers: inputs and the outputs ACE's own code gives for them, which the
server's tests replay against the port (a test module that does so says `Vectors:`). They are
regenerated during the upstream sync by the ACE vector harness, among the server's tools, never
edited by hand. The protocol recordings the
client's and the server's tests share are in the repository's top-level `fixtures/`.
