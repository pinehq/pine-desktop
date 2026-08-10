ARG UBUNTU_VERSION=24.04
FROM ubuntu:${UBUNTU_VERSION}

RUN apt-get update \
    && DEBIAN_FRONTEND=noninteractive apt-get install --yes --no-install-recommends \
        build-essential \
        ca-certificates \
        git \
        libadwaita-1-dev \
        libgtk-4-dev \
        libgtksourceview-5-dev \
        libvte-2.91-gtk4-dev \
        pkg-config \
        rustup \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/*

RUN rustup default 1.96.1 \
    && rustup component add clippy rustfmt

WORKDIR /workspace

CMD ["cargo", "test", "--workspace"]
