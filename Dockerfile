FROM debian:stable-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
      bash \
      coreutils \
      jq \
      make \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /workspace
COPY . /workspace

CMD ["make", "matrix"]
