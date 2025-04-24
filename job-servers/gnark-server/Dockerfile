FROM ubuntu:22.04

ENV DEBIAN_FRONTEND=noninteractive \
    GO111MODULE=on \
    GOPATH=/go \
    PATH=$PATH:/go/bin:/usr/local/go/bin

RUN apt-get update && apt-get install -y \
    wget \
    git \
    curl \
    iproute2 \
    && rm -rf /var/lib/apt/lists/*

RUN wget https://go.dev/dl/go1.21.7.linux-amd64.tar.gz \
    && tar -C /usr/local -xzf go1.21.7.linux-amd64.tar.gz \
    && rm go1.21.7.linux-amd64.tar.gz

RUN apt-get update && apt-get install -y \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY go.mod go.sum ./
RUN go mod download

COPY . .

RUN go build -o main ./main.go

ENTRYPOINT ["./main"]