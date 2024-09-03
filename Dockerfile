FROM --platform=linux/amd64 debian:latest

RUN apt-get update && \
  apt-get install -y --no-install-recommends ca-certificates

RUN mkdir /open-epaper-gen
COPY target/x86_64-unknown-linux-gnu/release/open-epaper-gen /open-epaper-gen
ADD target/x86_64-unknown-linux-gnu/release/resources /open-epaper-gen/resources

WORKDIR /open-epaper-gen
