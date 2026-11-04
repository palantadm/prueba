"""Launch Lumos."""
import uvicorn
import os

if __name__ == "__main__":
    host = os.getenv("LUMOS_HOST", "0.0.0.0")
    port = int(os.getenv("LUMOS_PORT", "8080"))
    uvicorn.run(
        "backend.main:app",
        host=host,
        port=port,
        reload=False,
        log_level="info",
        app_dir=str(__import__("pathlib").Path(__file__).parent),
    )
