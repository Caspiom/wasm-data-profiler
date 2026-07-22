"""FastAPI service that profiles a CSV with pandas.

This exists only so the Wasm engine has something to be measured against. It is
not part of the product: the browser never needs it, and the file you send here
does leave your machine, which the front end says plainly before sending.
"""

from __future__ import annotations

import platform

import pandas as pd
from fastapi import FastAPI, File, HTTPException, UploadFile
from fastapi.middleware.cors import CORSMiddleware

from app import models
from app.profiler import profile_csv

VERSIONS = {
    "python": platform.python_version(),
    "pandas": pd.__version__,
}

app = FastAPI(
    title="mirante comparison API",
    description="Profiles a CSV with pandas, in the format the Wasm engine returns.",
    version="0.1.0",
)

# The front end is served from a different origin than this service, whether
# that is localhost during development or GitHub Pages in production.
app.add_middleware(
    CORSMiddleware,
    allow_origin_regex=r"http://localhost:\d+|http://127\.0\.0\.1:\d+|https://.*\.github\.io",
    allow_methods=["POST", "GET"],
    allow_headers=["*"],
)


@app.get("/health")
def health() -> dict[str, str | dict[str, str]]:
    return {"status": "ok", "engine": "pandas", "versions": VERSIONS}


@app.post("/profile", response_model=models.ProfileResponse)
async def profile(file: UploadFile = File(...)) -> models.ProfileResponse:  # noqa: B008
    """Profile an uploaded CSV.

    The returned timings cover parsing and aggregation only. Time spent on the
    network and on multipart decoding is deliberately excluded here so the
    caller can measure and report it separately — it is the largest part of the
    difference against Wasm, and burying it would flatter this service.
    """
    raw = await file.read()
    try:
        result, timings = profile_csv(raw)
    except ValueError as error:
        raise HTTPException(status_code=422, detail=str(error)) from error

    return models.ProfileResponse(
        profile=result,
        timings=timings,
        engine="pandas",
        versions=VERSIONS,
    )
