import { HorizonStreamService } from "../src/services/horizon-stream.service";

describe("HorizonStreamService URL generation", () => {
  it("builds an events URL without account filter", () => {
    const service = new HorizonStreamService();
    const url = service.buildEventsUrl("123");

    expect(url).toContain("/events?");
    expect(url).toContain("type=contract");
    expect(url).toContain("cursor=123");
    expect(url).not.toContain("account=");
  });

  it("builds an events URL with account filter for wallet streaming", () => {
    const service = new HorizonStreamService();
    const url = service.buildEventsUrl("456", "GUSERWALLET123");

    expect(url).toContain("cursor=456");
    expect(url).toContain("account=GUSERWALLET123");
  });
});
