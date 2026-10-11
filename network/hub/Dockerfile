# The Dereth Network's hub as a container: the release's Linux binary on a distroless base (glibc,
# CA roots, no shell), running as an unprivileged user. Built from a folder that holds the binary
# for each platform and an empty data folder:
#
#   bin/amd64/dereth-network-hub
#   bin/arm64/dereth-network-hub
#   data/
#
# The release workflow lays it out from its archives (network/hub/DEPLOY.md, section 7, by hand).
# The base is pinned by digest: gcr.io/distroless/cc-debian12:nonroot.
FROM gcr.io/distroless/cc-debian12:nonroot@sha256:9dac0a79194e45a7da0158a9c6da57b217585af0786db3845d1f0ec1a0dd182f

ARG TARGETARCH
COPY bin/${TARGETARCH}/dereth-network-hub /usr/local/bin/dereth-network-hub
# The registry's folder, owned by the user the hub runs as (distroless's nonroot).
COPY --chown=65532:65532 data /data

LABEL org.opencontainers.image.title="dereth-network-hub" \
      org.opencontainers.image.description="The Dereth Network's hub: the world registry and the links between worlds" \
      org.opencontainers.image.licenses="AGPL-3.0-only"

USER 65532:65532
ENV DERETH_HUB_DB=/data/hub.sqlite
VOLUME ["/data"]
EXPOSE 8650
ENTRYPOINT ["/usr/local/bin/dereth-network-hub"]
# All addresses; the hub refuses to start until it is told its proxy's own address
# (--trusted-proxy), so that it never trusts a whole network. DEPLOY.md, section 7, has the rest.
CMD ["serve", "--listen", "0.0.0.0:8650"]
