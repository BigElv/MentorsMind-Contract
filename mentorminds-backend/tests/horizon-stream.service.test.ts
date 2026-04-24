import { HorizonStreamService } from "../src/services/horizon-stream.service";

describe("HorizonStreamService", () => {
  const originalPlatform = process.env.PLATFORM_STELLAR_ACCOUNT;
  const originalExtra = process.env.HORIZON_PLATFORM_EXTRA_ACCOUNTS;

  afterEach(() => {
    if (originalPlatform === undefined) {
      delete process.env.PLATFORM_STELLAR_ACCOUNT;
    } else {
      process.env.PLATFORM_STELLAR_ACCOUNT = originalPlatform;
    }
    if (originalExtra === undefined) {
      delete process.env.HORIZON_PLATFORM_EXTRA_ACCOUNTS;
    } else {
      process.env.HORIZON_PLATFORM_EXTRA_ACCOUNTS = originalExtra;
    }
  });

  it("buildEventsUrl omits account filter when no account is passed", () => {
    const service = new HorizonStreamService();
    const url = service.buildEventsUrl("99");

    expect(url).toContain("/events?");
    expect(url).toContain("type=contract");
    expect(url).toContain("cursor=99");
    expect(url).not.toContain("account=");
  });

  it("buildEventsUrl includes account when scoping to platform ingress", () => {
    const service = new HorizonStreamService();
    const url = service.buildEventsUrl("100", "GPLATFORMXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX");

    expect(url).toContain("cursor=100");
    expect(url).toContain(
      "account=GPLATFORMXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX"
    );
  });

  it("getPlatformAccounts returns primary then extras, deduped", () => {
    process.env.PLATFORM_STELLAR_ACCOUNT = "GAAA";
    process.env.HORIZON_PLATFORM_EXTRA_ACCOUNTS = " GBBB , GAAA ,GCC ";

    const service = new HorizonStreamService();
    expect(service.getPlatformAccounts()).toEqual(["GAAA", "GBBB", "GCC"]);
  });
});
