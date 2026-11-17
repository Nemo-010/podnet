# The image that builds the static Linux binaries of podnet (release.yml). This
# line is the one place that names it; the workflow reads it. The digest is
# that of the multi-platform index (linux/amd64 and linux/arm64), so two runs
# of one commit use one compiler, on each architecture.
FROM docker.io/library/rust:1-alpine@sha256:2cccd8b212b37dad15a555589c1b6787da4ab913fe24d8a9b2d99d3498869409
