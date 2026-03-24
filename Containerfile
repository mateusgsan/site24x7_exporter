# Use debian:stable-slim as the base image.
# The "stable" tag always points to the latest Debian stable release,
# ensuring the image is rebuilt with up-to-date system packages on every CI run.
# "slim" reduces the attack surface by excluding unnecessary packages.
#
# NOTE: No RUN instructions are used here intentionally.
# The podman build steps in the CI workflow run on an amd64 host and build
# cross-platform manifests (arm64, arm/v7) without QEMU emulation.
# Any RUN instruction would attempt to execute a foreign-arch binary on the
# host kernel, causing "Exec format error". All required packages
# (ca-certificates) are already present in debian:stable-slim since Debian 12.
FROM docker.io/debian:bookwarm-slim

# Copy the pre-compiled binary produced by the Rust cross-compilation step.
COPY --chmod=755 site24x7_exporter /app/

# Run as a non-root user for security.
# UID/GID 65534 is the standard "nobody" user present in all Debian images,
# avoiding the need for a RUN useradd instruction.
USER 65534

ENTRYPOINT ["/app/site24x7_exporter"]
