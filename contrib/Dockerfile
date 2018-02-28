# Cross compilation environment for librespot
# Build the docker image from the root of the project with the following command :
# $ docker build -t librespot-cross -f contrib/Dockerfile .
#
# The resulting image can be used to build librespot for linux x86_64, i386 and armhf(with support for armv6hf), armel, mipsel, aarch64
# $ docker run -v ${pwd}/crosssbuild:/build librespot-cross
#
# The compiled binaries will be located in ${pwd}/crosssbuild
#
# If only one architecture is desired, cargo can be invoked directly with the appropriate options :
# $ docker run -v ${pwd}/crosssbuild:/build librespot-cross cargo build --release --no-default-features --features "alsa-backend"
# $ docker run -v ${pwd}/crosssbuild:/build librespot-cross cargo build --release --target i686-unknown-linux-gnu --no-default-features --features alsa-backend
# $ docker run -v ${pwd}/crosssbuild:/build librespot-cross cargo build --release --target arm-unknown-linux-gnueabihf --no-default-features --features "alsa-backend"
# $ docker run -v ${pwd}/crosssbuild:/build librespot-cross cargo build --release --target arm-unknown-linux-gnueabi --no-default-features --features "alsa-backend"
# $ docker run -v ${pwd}/crosssbuild:/build librespot-cross contrib/docker-build-pi-armv6hf.sh

FROM debian:stretch

RUN dpkg --add-architecture arm64
RUN dpkg --add-architecture armhf
RUN dpkg --add-architecture armel
RUN dpkg --add-architecture mipsel
RUN dpkg --add-architecture i386

RUN apt-get update

RUN apt-get install -y curl git build-essential crossbuild-essential-arm64 crossbuild-essential-armel crossbuild-essential-armhf crossbuild-essential-mipsel
RUN apt-get install -y libasound2-dev libasound2-dev:arm64 libasound2-dev:armel libasound2-dev:armhf libasound2-dev:mipsel libasound2-dev:i386
RUN apt-get install -y g++-multilib gcc-multilib

RUN curl https://sh.rustup.rs -sSf | sh -s -- -y
ENV PATH="/root/.cargo/bin/:${PATH}"
RUN rustup target add aarch64-unknown-linux-gnu
RUN rustup target add arm-unknown-linux-gnueabi
RUN rustup target add arm-unknown-linux-gnueabihf
RUN rustup target add mipsel-unknown-linux-gnu
RUN rustup target add i686-unknown-linux-gnu

RUN mkdir /build && \
    mkdir /pi-tools && \
    curl -L https://github.com/raspberrypi/tools/archive/648a6eeb1e3c2b40af4eb34d88941ee0edeb3e9a.tar.gz | tar xz --strip-components 1 -C /pi-tools && \
    mkdir /.cargo && \
    echo '[target.aarch64-unknown-linux-gnu]\nlinker = "aarch64-linux-gnu-gcc"' > /.cargo/config && \
    echo '[target.arm-unknown-linux-gnueabihf]\nlinker = "arm-linux-gnueabihf-gcc"' >> /.cargo/config && \
    echo '[target.arm-unknown-linux-gnueabi]\nlinker = "arm-linux-gnueabi-gcc"' >> /.cargo/config && \
    echo '[target.mipsel-unknown-linux-gnu]\nlinker = "mipsel-linux-gnu-gcc"' >> /.cargo/config

ENV CARGO_TARGET_DIR /build
ENV CARGO_HOME /build/cache

ADD . /src
WORKDIR /src
CMD ["/src/contrib/docker-build.sh"]
