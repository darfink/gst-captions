# Build and inspect the captions elements without installing GStreamer locally.
#
#   docker build -t gst-captions .
#   docker run --rm gst-captions
#
# Ubuntu 26.04 is the base because it packages a recent GStreamer alongside
# the C++ toolchain the transcriber's CPU backend builds with.
FROM ubuntu:26.04 AS builder

ARG DEBIAN_FRONTEND=noninteractive
ARG RUST_VERSION=1.98.0

RUN apt-get update && \
    apt-get install -y --no-install-recommends \
      build-essential \
      ca-certificates \
      cmake \
      curl \
      git \
      libgstreamer1.0-dev \
      libgstreamer-plugins-base1.0-dev \
      pkg-config \
    && rm -rf /var/lib/apt/lists/*

RUN curl -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain "${RUST_VERSION}" --profile minimal
ENV PATH=/root/.cargo/bin:${PATH}

WORKDIR /work

# Dependencies first, so editing the elements does not rebuild the world.
COPY Cargo.toml Cargo.lock ./
COPY gst-captions/Cargo.toml gst-captions/build.rs ./gst-captions/
RUN mkdir -p gst-captions/src && echo '' > gst-captions/src/lib.rs && cargo build --release || true

COPY gst-captions/src ./gst-captions/src
RUN touch gst-captions/src/lib.rs && cargo build --release --locked

FROM ubuntu:26.04 AS runtime

ARG DEBIAN_FRONTEND=noninteractive

RUN apt-get update && \
    apt-get install -y --no-install-recommends \
      gstreamer1.0-plugins-bad \
      gstreamer1.0-plugins-base \
      gstreamer1.0-plugins-good \
      gstreamer1.0-tools \
    && rm -rf /var/lib/apt/lists/*

# Not /usr/lib/gstreamer-1.0: Debian and Ubuntu put the system plugin directory
# under a multiarch path, so a plain /usr/lib drop is never scanned.
COPY --from=builder /work/target/release/libgstcaptions.so /usr/local/lib/gstreamer-1.0/
ENV GST_PLUGIN_PATH=/usr/local/lib/gstreamer-1.0

# Fail the build rather than ship an image whose plugins do not load.
RUN gst-inspect-1.0 captionsflvmux > /dev/null && gst-inspect-1.0 captionsrollup > /dev/null && gst-inspect-1.0 captionstranscriber > /dev/null

# Inspect the elements. Override the command to build your own
# pipeline around them.
CMD ["gst-inspect-1.0", "captionsflvmux", "captionsrollup", "captionstranscriber"]
