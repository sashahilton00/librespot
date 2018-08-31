# Cross compilation environment for librespot
# Build the docker image from the root of the project with the following command :
# $ docker build -t librespot-cross -f contrib/Dockerfile .
#
# The resulting image can be used to build librespot for linux x86_64, armhf(with support for armv6hf), armel, mipsel, aarch64
# $ docker run -v /tmp/librespot-build:/build librespot-cross
#
# The compiled binaries will be located in /tmp/librespot-build
#
# If only one architecture is desired, cargo can be invoked directly with the appropriate options :
# $ docker run -v /tmp/librespot-build:/build librespot-cross cargo build --release --no-default-features --features "alsa-backend"
# $ docker run -v /tmp/librespot-build:/build librespot-cross cargo build --release --target arm-unknown-linux-gnueabihf --no-default-features --features "alsa-backend"
# $ docker run -v /tmp/librespot-build:/build librespot-cross cargo build --release --target arm-unknown-linux-gnueabi --no-default-features --features "alsa-backend"
# $ docker run -v /tmp/librespot-build:/build librespot-cross contrib/docker-build-pi-armv6hf.sh

FROM debian:stretch

RUN dpkg --add-architecture arm64
RUN dpkg --add-architecture armhf
RUN dpkg --add-architecture armel
RUN dpkg --add-architecture mipsel
RUN apt-get update

RUN apt-get install -y --no-install-recommends \
    build-essential ca-certificates curl git \
    crossbuild-essential-arm64 crossbuild-essential-armel crossbuild-essential-armhf crossbuild-essential-mipsel
RUN apt-get install -y libasound2-dev libasound2-dev:arm64 libasound2-dev:armel libasound2-dev:armhf libasound2-dev:mipsel

RUN curl https://sh.rustup.rs -sSf | sh -s -- -y
ENV PATH="/root/.cargo/bin/:${PATH}"
RUN rustup target add aarch64-unknown-linux-gnu
RUN rustup target add arm-unknown-linux-gnueabi
RUN rustup target add arm-unknown-linux-gnueabihf
RUN rustup target add mipsel-unknown-linux-gnu

RUN mkdir /.cargo && \
    echo '[target.aarch64-unknown-linux-gnu]\nlinker = "aarch64-linux-gnu-gcc"' > /.cargo/config && \
    echo '[target.arm-unknown-linux-gnueabihf]\nlinker = "arm-linux-gnueabihf-gcc"' >> /.cargo/config && \
    echo '[target.arm-unknown-linux-gnueabi]\nlinker = "arm-linux-gnueabi-gcc"' >> /.cargo/config && \
    echo '[target.mipsel-unknown-linux-gnu]\nlinker = "mipsel-linux-gnu-gcc"' >> /.cargo/config

# Get raspbian cross compiler tools
RUN mkdir /build && \
    mkdir /pi-tools && cd /pi-tools && git init && git config core.sparseCheckout true && \
    git remote add origin https://github.com/raspberrypi/tools.git && \
    echo "arm-bcm2708/arm-bcm2708hardfp-linux-gnueabi/*" >> .git/info/sparse-checkout && \
    echo "arm-bcm2708/gcc-linaro-arm-linux-gnueabihf-raspbian-x64/*" >> .git/info/sparse-checkout && \
    git pull --depth=1 origin master

RUN apt-get install -y pkg-config

ENV CARGO_TARGET_DIR /build
ENV CARGO_HOME /build/cache

ADD . /src
WORKDIR /src
CMD ["/src/contrib/docker-build.sh"]
