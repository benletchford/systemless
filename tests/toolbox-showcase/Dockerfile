FROM golang:1.21-bookworm@sha256:c6a5b9308b3f3095e8fde83c8bf4d68bd101fce606c1a0a1394522542509dda9 AS builder

ARG MPS_COMMIT=e2b8c66e08caaf31e527e3dbecacd0926e11a47c
WORKDIR /opt/mps
RUN git clone https://github.com/elliotnunn/mps.git . \
    && git checkout --detach "${MPS_COMMIT}" \
    && go build -trimpath -o /usr/local/bin/mps ./

FROM debian:bookworm-slim@sha256:88200866dfff7ea7f5cbcb6ec7c8a701889efe6fe859fe64d6990e4b07ea4171

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl python3 python3-pip \
    && rm -rf /var/lib/apt/lists/* \
    && pip3 install --no-cache-dir --break-system-packages macresources==1.2

COPY --from=builder /usr/local/bin/mps /usr/local/bin/mps

ARG MPW_INSTALL_URL=https://sonixwave.com/apple/developer/Tool_Chest/Core_Mac_OS_Tools/MPW_etc./MPW-GM_Images/MPW-GM.img.bin
ARG MPW_SHA256=99bbfa95bb9800c8ffc572fce6d72e561f012331c5c623fa45f732502b6fa872
RUN curl --fail --location --silent --show-error "${MPW_INSTALL_URL}" -o /tmp/MPW-GM.img.bin \
    && echo "${MPW_SHA256}  /tmp/MPW-GM.img.bin" | sha256sum --check - \
    && mps -install /tmp/MPW-GM.img.bin \
    && rm /tmp/MPW-GM.img.bin

WORKDIR /workspace
ENTRYPOINT ["mps"]
