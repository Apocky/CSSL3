FROM python:3.12-slim

WORKDIR /app

# Install system deps if needed (for your real CSL/CSSL parser later)
RUN apt-get update && apt-get install -y --no-install-recommends \
    git \
    && rm -rf /var/lib/apt/lists/*

COPY requirements.txt .
RUN pip install --no-cache-dir -r requirements.txt

COPY harness.py .
COPY project_tools.py* . 2>/dev/null || true

# Create log dir
RUN mkdir -p /app/logs

EXPOSE 8080

CMD ["python", "harness.py"]
