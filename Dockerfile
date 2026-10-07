# Payback reimplementation workbench
# Reverse-engineering + Rust/Bevy toolchain for headless agents.
#
# Build:  docker build -t payback-re .
# Run:    docker run -it --rm \
#           -v "$PWD/extracted":/work/game-data:ro \
#           -v "$PWD/analysis":/work/analysis \
#           -v "$PWD":/work/impl \
#           payback-re
#
# game-data is the unpacked CD + updates (see tools/extract.py).
# Original game files are mounted read-only and never baked into the image.

FROM ubuntu:24.04

ARG DEBIAN_FRONTEND=noninteractive

# Pinned versions — bump deliberately.
ARG GHIDRA_VERSION=12.1.4
ARG GHIDRA_SHA256=ddac49f903da9d5bac833e5cc79395098b9c33cfd3279be5f31bd00387d2d4db
ARG KAITAI_VERSION=0.10

# ---------------------------------------------------------------------------
# 1. System packages
# ---------------------------------------------------------------------------
RUN apt-get update && apt-get install -y --no-install-recommends \
    # basics
    ca-certificates curl wget git unzip jq file xxd less vim-tiny \
    build-essential pkg-config clang lld cmake \
    python3 python3-venv python3-dev python3-pip \
    # Ghidra runtime
    openjdk-21-jdk-headless \
    # CLI reverse-engineering
    radare2 binwalk lhasa p7zip-full \
    # Amiga emulator (run headless under Xvfb when needed)
    fs-uae xvfb xauth \
    # image comparison for verification
    imagemagick \
    # Bevy build + headless rendering dependencies
    libasound2-dev libudev-dev libwayland-dev libxkbcommon-dev \
    libx11-dev libxcursor-dev libxi-dev libxrandr-dev \
    libvulkan1 mesa-vulkan-drivers vulkan-tools \
 && rm -rf /var/lib/apt/lists/*

# ---------------------------------------------------------------------------
# 2. Ghidra (asset URL resolved via GitHub API, then checksum-verified)
# ---------------------------------------------------------------------------
RUN set -eux; \
    url=$(curl -fsSL "https://api.github.com/repos/NationalSecurityAgency/ghidra/releases/tags/Ghidra_${GHIDRA_VERSION}_build" \
          | jq -r '.assets[] | select(.name | endswith(".zip")) | .browser_download_url'); \
    curl -fsSL -o /tmp/ghidra.zip "$url"; \
    echo "${GHIDRA_SHA256}  /tmp/ghidra.zip" | sha256sum -c -; \
    unzip -q /tmp/ghidra.zip -d /opt; \
    mv /opt/ghidra_${GHIDRA_VERSION}_PUBLIC /opt/ghidra; \
    rm /tmp/ghidra.zip

ENV GHIDRA_INSTALL_DIR=/opt/ghidra
ENV PATH="/opt/ghidra/support:${PATH}"
# analyzeHeadless is now on PATH

# ---------------------------------------------------------------------------
# 3. Kaitai Struct compiler
# ---------------------------------------------------------------------------
RUN curl -fsSL -o /tmp/ksc.deb \
      "https://github.com/kaitai-io/kaitai_struct_compiler/releases/download/${KAITAI_VERSION}/kaitai-struct-compiler_${KAITAI_VERSION}_all.deb" \
 && apt-get update && apt-get install -y --no-install-recommends /tmp/ksc.deb \
 && rm -rf /tmp/ksc.deb /var/lib/apt/lists/*

# ---------------------------------------------------------------------------
# 4. Non-root user for agents
# ---------------------------------------------------------------------------
RUN useradd -m -s /bin/bash re \
 && mkdir -p /work/game-data /work/analysis /work/impl \
 && chown -R re:re /work
USER re
WORKDIR /work

# ---------------------------------------------------------------------------
# 5. Python venv: PyGhidra, disassembly/emulation, Amiga tools, imaging
# ---------------------------------------------------------------------------
ENV VENV=/home/re/venv
RUN python3 -m venv $VENV
ENV PATH="${VENV}/bin:${PATH}"

RUN pip install --no-cache-dir --upgrade pip wheel \
 && pip install --no-cache-dir \
      -f /opt/ghidra/Ghidra/Features/PyGhidra/pypkg/dist pyghidra \
 && pip install --no-cache-dir \
      capstone unicorn \
      amitools machine68k \
      pyelftools \
      kaitaistruct \
      numpy pillow \
      rich

# ---------------------------------------------------------------------------
# 6. Rust + Bevy-side tooling
# ---------------------------------------------------------------------------
RUN curl -fsSL https://sh.rustup.rs | sh -s -- -y --profile minimal \
      --component clippy,rustfmt
ENV PATH="/home/re/.cargo/bin:${PATH}"

RUN cargo install --locked bacon cargo-insta

# Faster linking for Bevy builds
RUN mkdir -p /home/re/.cargo && printf '%s\n' \
    '[target.x86_64-unknown-linux-gnu]' \
    'linker = "clang"' \
    'rustflags = ["-C", "link-arg=-fuse-ld=lld"]' \
    > /home/re/.cargo/config.toml

# Software Vulkan (lavapipe) so Bevy can render headless to PNG
ENV VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.x86_64.json
ENV WGPU_BACKEND=vulkan

# ---------------------------------------------------------------------------
# 7. Sanity check on build
# ---------------------------------------------------------------------------
RUN analyzeHeadless 2>&1 | head -n 3 || true; \
    r2 -v | head -n 1; \
    ksc --version; \
    hunktool --help > /dev/null && echo "amitools ok"; \
    python -c "import capstone, unicorn, pyghidra; print('python RE libs ok')"; \
    cargo --version

CMD ["/bin/bash"]
