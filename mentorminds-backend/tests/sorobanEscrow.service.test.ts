import { validateStellarAmount } from "../src/services/sorobanEscrow.service";

describe("validateStellarAmount", () => {
  it("accepts valid positive amounts", () => {
    expect(() => validateStellarAmount("100")).not.toThrow();
    expect(() => validateStellarAmount("0.1")).not.toThrow();
    expect(() => validateStellarAmount("1000.1234567")).not.toThrow();
  });

  it("rejects zero", () => {
    expect(() => validateStellarAmount("0")).toThrow("must be greater than 0");
  });

  it("rejects negative amounts", () => {
    expect(() => validateStellarAmount("-50")).toThrow("must be a positive decimal number");
  });

  it("rejects non-numeric strings", () => {
    expect(() => validateStellarAmount("abc")).toThrow("must be a positive decimal number");
    expect(() => validateStellarAmount("")).toThrow("must be a positive decimal number");
  });

  it("rejects amounts with more than 7 decimal places", () => {
    expect(() => validateStellarAmount("1.12345678")).toThrow("must have at most 7 decimal places");
    expect(() => validateStellarAmount("999999999999999.99999999")).toThrow("must have at most 7 decimal places");
  });

  it("accepts amounts with exactly 7 decimal places", () => {
    expect(() => validateStellarAmount("1.1234567")).not.toThrow();
  });

  it("rejects amounts exceeding max Stellar amount", () => {
    expect(() => validateStellarAmount("922337203685.4775808")).toThrow("exceeds maximum Stellar amount");
    expect(() => validateStellarAmount("999999999999999.9999999")).toThrow("exceeds maximum Stellar amount");
  });

  it("accepts the max Stellar amount", () => {
    expect(() => validateStellarAmount("922337203685.4775807")).not.toThrow();
  });

  it("throws errors with statusCode 400", () => {
    try {
      validateStellarAmount("0");
    } catch (error: any) {
      expect(error.statusCode).toBe(400);
    }

    try {
      validateStellarAmount("-10");
    } catch (error: any) {
      expect(error.statusCode).toBe(400);
    }

    try {
      validateStellarAmount("1.12345678");
    } catch (error: any) {
      expect(error.statusCode).toBe(400);
    }

    try {
      validateStellarAmount("999999999999999.9999999");
    } catch (error: any) {
      expect(error.statusCode).toBe(400);
    }
  });
});
