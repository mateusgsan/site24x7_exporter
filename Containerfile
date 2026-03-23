# Use debian:stable-slim as the base image.
# The "stable" tag always points to the latest Debian stable release,
# ensuring the image is rebuilt with up-to-date system packages on every CI run.
# "slim" reduces the attack surface by excluding unnecessary packages.
FROM docker.io/debian:stable-slim

# Install CA certificates required for HTTPS calls to the Site24x7 API,
# then clean up the apt cache to keep the image layer small.
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Create a dedicated non-root user and group to run the exporter.
# Running as root inside a container is a security anti-pattern.
RUN groupadd --system exporter \
    && useradd --system --gid exporter --no-create-home exporter

COPY --chmod=755 site24x7_exporter /app/

USER exporter
ENTRYPOINT ["/app/site24x7_exporter"]
