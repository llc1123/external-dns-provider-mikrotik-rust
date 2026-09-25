package conformance

import (
	"net"
	"net/http"
	"os"
	"strconv"
	"strings"
	"testing"
	"time"
)

func routerOSTarget(t *testing.T) (string, *routerOSFake) {
	baseURL := os.Getenv("CONFORMANCE_ROUTEROS_BASEURL")
	username := os.Getenv("CONFORMANCE_ROUTEROS_USERNAME")
	password := os.Getenv("CONFORMANCE_ROUTEROS_PASSWORD")
	if baseURL == "" && username == "" && password == "" {
		fake := newRouterOSFake(t)
		t.Setenv("CONFORMANCE_ROUTEROS_USERNAME", "test")
		t.Setenv("CONFORMANCE_ROUTEROS_PASSWORD", "test")
		return fake.server.URL, fake
	}
	if baseURL == "" || username == "" || password == "" {
		t.Fatal("CONFORMANCE_ROUTEROS_BASEURL, _USERNAME, and _PASSWORD must all be set for external mode")
	}
	return strings.TrimRight(baseURL, "/"), nil
}

func routerOSUsername(t *testing.T) string {
	value := os.Getenv("CONFORMANCE_ROUTEROS_USERNAME")
	if value == "" {
		return "test"
	}
	return value
}

func routerOSPassword(t *testing.T) string {
	value := os.Getenv("CONFORMANCE_ROUTEROS_PASSWORD")
	if value == "" {
		return "test"
	}
	return value
}

func reservePort(t *testing.T) string {
	listener, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	port := listener.Addr().(*net.TCPAddr).Port
	_ = listener.Close()
	return strconv.Itoa(port)
}

func waitReady(t *testing.T, url string) {
	for deadline := time.Now().Add(10 * time.Second); time.Now().Before(deadline); {
		if response, err := http.Get(url); err == nil {
			_ = response.Body.Close()
			if response.StatusCode < 500 {
				return
			}
		}
		time.Sleep(25 * time.Millisecond)
	}
	t.Fatalf("webhook did not become ready: %s", url)
}
