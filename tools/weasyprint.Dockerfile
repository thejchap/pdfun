FROM python:3.12-slim

# WeasyPrint needs Pango/HarfBuzz/Fontconfig at runtime; we bundle a fixed
# font set so that reference snapshots are reproducible across machines.
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        libpango-1.0-0 \
        libpangoft2-1.0-0 \
        libharfbuzz0b \
        libfontconfig1 \
        fonts-liberation \
        fonts-dejavu-core \
    && rm -rf /var/lib/apt/lists/*

ARG WEASYPRINT_VERSION=65.0
RUN pip install --no-cache-dir "weasyprint==${WEASYPRINT_VERSION}"

LABEL org.opencontainers.image.title="pdfun-weasyprint" \
      org.opencontainers.image.description="Pinned WeasyPrint reference renderer for pdfun visual regression" \
      org.opencontainers.image.source="https://github.com/thejchap/pdfun" \
      pdfun.weasyprint.version="${WEASYPRINT_VERSION}"

WORKDIR /work
ENTRYPOINT ["weasyprint"]
