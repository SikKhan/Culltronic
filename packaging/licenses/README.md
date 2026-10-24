# Supplemental dependency license texts

These upstream texts fill omissions in the published crate archives. `packages.json`
pins the applicable crate names and versions; `sources.json` records immutable
upstream source URLs. The AccessKit license texts were also verified identical at
`1bbcf100942bac96c2c3a4a91cb67b0b20201a24` (consumer/windows/winit versions in
Cargo.lock); AUTHORS comes from that revision.

The packager additionally collects nested crate notices and font license texts,
including egui's Hack, Noto, Ubuntu and emoji fonts. The packaging test fails if
a dependency lacks both bundled texts and a reviewed supplement. Review these
entries when updating Cargo.lock rather than falling back to a license identifier.
