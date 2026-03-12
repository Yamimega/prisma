import { NextRequest, NextResponse } from "next/server";

const MGMT_API_URL = process.env.MGMT_API_URL || "http://127.0.0.1:9090";
const MGMT_API_TOKEN = process.env.MGMT_API_TOKEN || "";

async function proxyRequest(
  req: NextRequest,
  { params }: { params: Promise<{ path: string[] }> }
): Promise<NextResponse> {
  const { path } = await params;

  // Reject WebSocket upgrade requests — WS proxying is handled client-side
  if (req.headers.get("upgrade")?.toLowerCase() === "websocket") {
    return NextResponse.json(
      { error: "WebSocket proxying is not supported on this endpoint" },
      { status: 501 }
    );
  }

  const targetPath = path.join("/");
  const url = new URL(targetPath, MGMT_API_URL);

  // Forward query parameters
  req.nextUrl.searchParams.forEach((value, key) => {
    url.searchParams.append(key, value);
  });

  const headers = new Headers();
  // Forward select headers from the incoming request
  const forwardHeaders = ["content-type", "accept", "x-request-id"];
  for (const name of forwardHeaders) {
    const value = req.headers.get(name);
    if (value) {
      headers.set(name, value);
    }
  }

  // Add Bearer token for the management API
  if (MGMT_API_TOKEN) {
    headers.set("Authorization", `Bearer ${MGMT_API_TOKEN}`);
  }

  const fetchInit: RequestInit = {
    method: req.method,
    headers,
  };

  // Forward request body for methods that support it
  if (req.method !== "GET" && req.method !== "HEAD") {
    const body = await req.arrayBuffer();
    if (body.byteLength > 0) {
      fetchInit.body = body;
    }
  }

  try {
    const upstream = await fetch(url.toString(), fetchInit);

    const responseHeaders = new Headers();
    // Forward select response headers
    const returnHeaders = ["content-type", "x-request-id"];
    for (const name of returnHeaders) {
      const value = upstream.headers.get(name);
      if (value) {
        responseHeaders.set(name, value);
      }
    }

    const responseBody = await upstream.arrayBuffer();
    return new NextResponse(responseBody, {
      status: upstream.status,
      statusText: upstream.statusText,
      headers: responseHeaders,
    });
  } catch (err) {
    console.error("Proxy error:", err);
    return NextResponse.json(
      { error: "Failed to connect to management API" },
      { status: 502 }
    );
  }
}

export const GET = proxyRequest;
export const POST = proxyRequest;
export const PUT = proxyRequest;
export const PATCH = proxyRequest;
export const DELETE = proxyRequest;
