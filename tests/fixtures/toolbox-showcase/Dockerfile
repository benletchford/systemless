FROM golang:1.25.6-bookworm AS mps-builder

ARG MPS_REV=e2b8c66e08caaf31e527e3dbecacd0926e11a47c
WORKDIR /opt/mps
RUN mkdir -p /out \
    && git clone https://github.com/elliotnunn/mps.git . \
    && git checkout --detach "${MPS_REV}" \
    && go build -trimpath -ldflags='-buildid=' -o /out/mps ./

FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates python3 python3-pip \
    && rm -rf /var/lib/apt/lists/*
RUN pip3 install --break-system-packages --no-cache-dir macresources==1.2

COPY --from=mps-builder /out/mps /usr/local/bin/mps

ARG MPW_INSTALL_URL=https://sonixwave.com/apple/developer/Tool_Chest/Core_Mac_OS_Tools/MPW_etc./MPW-GM_Images/MPW-GM.img.bin
RUN mps -install "${MPW_INSTALL_URL}"

WORKDIR /workspace
ENTRYPOINT ["mps"]
