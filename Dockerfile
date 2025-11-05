FROM alpine:3.21

ARG TARGETPLATFORM

ENV RUST_LOG=info

WORKDIR /app
RUN adduser -D dephy --uid 1573 && chown -R dephy:dephy /app

COPY ./${TARGETPLATFORM}/dstack-backend /usr/bin/dstack-backend

USER dephy
ENTRYPOINT [ "/usr/bin/dstack-backend" ]