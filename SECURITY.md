# Security Policy

**Version**: 1.0  
**Effective Date**: March 25, 2026  
**Last Updated**: March 25, 2026

---

## 🛡️ Security Commitment

The MentorMinds team takes security seriously. We are committed to protecting the integrity and safety of our smart contracts and the assets they manage. We believe in **responsible disclosure** and appreciate the security research community's help in identifying vulnerabilities.

---

## 📋 Scope

This security policy applies to all MentorMinds smart contracts deployed on Stellar:

### In-Scope Contracts

| Contract                                                        | Status    | Deployment  |
| --------------------------------------------------------------- | --------- | ----------- |
| **Escrow Contract** (`escrow/src/lib.rs`)                       | Pre-audit | Testnet TBD |
| **Verification Contract** (`contracts/verification/src/lib.rs`) | Pre-audit | Testnet TBD |
| **MNT Token Contract** (`contracts/mnt-token/src/lib.rs`)       | Pre-audit | Testnet TBD |
| **Referral Contract** (`contracts/referral/src/lib.rs`)         | Pre-audit | Testnet TBD |

### Out-of-Scope

- Frontend web application (separate repository)
- Backend API services (separate repository)
- Third-party integrations
- Social engineering attacks
- Phishing or physical security attacks
- Attacks that require access to admin private keys

---

## 🔒 Responsible Disclosure Guidelines

### What is Responsible Disclosure?

Responsible disclosure means:

1. **Privately reporting** vulnerabilities to our security team
2. **Allowing reasonable time** for us to investigate and remediate
3. **Coordinating public disclosure** after a fix is available
4. **Acting in good faith** to protect users and the platform

### What We Ask

When you discover a potential vulnerability:

1. **Report immediately** via email to: `security@mentorminds.io`
2. **Include detailed information**:

   - Type of vulnerability
   - Full description of the issue
   - Step-by-step reproduction instructions
   - Affected contract addresses and versions
   - Potential impact assessment
   - Any suggested mitigations (if known)

3. **Provide proof of concept** (if possible):

   - Code snippets
   - Transaction hashes (testnet only)
   - Screenshots or videos

4. **Maintain confidentiality**:
   - Do not disclose publicly until we've had time to respond
   - Do not exploit the vulnerability beyond demonstration
   - Do not access funds you don't own

### What NOT to Do

❌ **Do NOT**:

- Exploit vulnerabilities on mainnet deployments
- Attempt to drain funds from escrows
- Launch denial-of-service attacks
- Test phishing campaigns against users
- Submit low-quality or automated reports
- Demand rewards or threaten disclosure

✅ **DO**:

- Act in good faith and with integrity
- Respect user.py and privacy of users
- Make a good-faith effort to avoid privacy violations
- Provide us a reasonable time to respond before any public discussion

---

## 🎯 Vulnerability Categories

### Critical Severity

Vulnerabilities that could result in:

- Direct loss of user funds
- Permanent locking of funds
- Unauthorized minting of tokens
- Complete protocol insolvency

**Examples**:

- Reentrancy leading to fund theft
- Authorization bypass allowing fund withdrawal
- Integer overflow enabling infinite minting
- Logic errors allowing double-spending

**Response Time**: 24 hours  
**Reward**: Up to $10,000 USD (or bounty program equivalent)

### High Severity

Vulnerabilities that could result in:

- Temporary fund freezing
- Manipulation of fee calculations
- Bypassing of critical business logic
- Significant service disruption

**Examples**:

- Fee calculation errors (>1% deviation)
- Dispute resolution manipulation
- Auto-release timing exploits
- Gas optimization attacks

**Response Time**: 48 hours  
**Reward**: Up to $5,000 USD

### Medium Severity

Vulnerabilities that could result in:

- Minor financial loss (<1% of affected funds)
- Information leakage
- Non-critical logic errors
- UI/UX confusion leading to user error

**Examples**:

- Off-by-one errors in counters
- Missing event emissions
- Edge cases in timestamp handling
- Symbol truncation issues

**Response Time**: 5 business days  
**Reward**: Up to $1,000 USD

### Low Severity

Issues that have minimal security impact:

- Best practice deviations
- Code quality issues
- Documentation errors
- Minor gas optimizations

**Examples**:

- Missing NatSpec comments
- Suboptimal storage layout
- Non-critical clippy warnings

**Response Time**: 10 business days  
**Reward**: Swag and public acknowledgment (optional)

### Not Eligible for Rewards

The following are generally not eligible for bug bounties:

- Already reported or known issues
- Issues in out-of-scope components
- Theoretical attacks without practical exploit
- Issues requiring admin key compromise
- Social engineering vulnerabilities
- Violations that we explicitly document as risks
- Issues already fixed in development branch

---

## 🏆 Bug Bounty Program

### Eligibility

- Open to security researchers worldwide
- Must be first to report the specific vulnerability
- Must follow responsible disclosure guidelines
- Cannot be MentorMinds team member or contractor
- Government employees and sanctioned entities excluded

### Reward Determination

Rewards are determined by:

1. **Severity** of the vulnerability
2. **Impact** on users and protocol
3. **Quality** of the report
4. **Exploitability** in current state

**Note**: Rewards are discretionary and may be adjusted based on:

- Quality and completeness of the report
- Cooperation during remediation
- Public disclosure behavior
- Previous contributions to the community

### Payment Methods

- Cryptocurrency (USDC, XLM, BTC, ETH)
- Bank transfer (for verified researchers)
- Equity grants (for exceptional, repeated contributions)

**Tax Responsibility**: Recipients are responsible for reporting rewards per local tax laws.

---

## 📊 Disclosure Timeline

```
Day 0:    Researcher submits private report
          ↓
Day 0-1:  Security team acknowledges receipt
          ↓
Day 1-3:  Initial triage and severity assessment
          ↓
Day 3-7:  Investigation and reproduction
          ↓
Day 7-14: Fix development and testing
          ↓
Day 14-21: Internal review and deployment to testnet
          ↓
Day 21-30: External audit (if critical/high)
          ↓
Day 30+:  Mainnet deployment and coordinated disclosure
```

### Expedited Timeline

For **Critical** vulnerabilities:

- Immediate escalation to CTO
- Daily status updates
- Emergency patch deployment if needed
- Coordinated disclosure within 7-14 days

### Delayed Disclosure

We may request delayed disclosure when:

- Exploit is actively being developed
- Patch requires complex coordination
- External dependencies need updating
- Regulatory considerations apply

Maximum delay: 90 days from initial report

---

## 🔐 Security Best Practices for Researchers

### Testing Guidelines

✅ **Safe Testing Environments**:

- Use local Soroban environment (`soroban env`)
- Deploy to testnet only with small amounts
- Never test on mainnet unless explicitly authorized
- Use provided test fixtures and snapshots

✅ **Recommended Tools**:

- Cargo fuzz testing
- Property-based testing (proptest)
- Static analysis (cargo-clippy, cargo-audit)
- Manual code review with annotation

❌ **Prohibited Activities**:

- Testing on production/mainnet contracts
- Interacting with other users' escrows
- Attempting to profit from discovered bugs
- Sharing exploit code with third parties

### Code Review Focus Areas

Based on our threat model, prioritize:

1. **Authorization Logic**:

   - `require_auth()` calls in privileged functions
   - Admin-only function guards
   - Multi-sig implementation (future)

2. **Financial Calculations**:

   - Fee computation (`platform_fee = amount * fee_bps / 10000`)
   - Dispute split calculations
   - Rounding and truncation behavior

3. **State Transitions**:

   - Escrow status machine (Active → Released/Refunded/Resolved)
   - Double-release prevention
   - Dispute resolution flow

4. **External Calls**:

   - Token contract interactions
   - Cross-contract calls (referral → MNT mint)
   - Reentrancy guards

5. **Edge Cases**:
   - Zero/negative amounts
   - Maximum values (i128::MAX)
   - Timestamp boundaries
   - Empty symbols and addresses

---

## 📞 Reporting Channels

### Primary Contact

**Email**: security@mentorminds.io  
**PGP Key**: [Available upon request]  
**Response Time**: Within 24 hours

### Alternative Contacts

For urgent matters only:

- **Telegram**: @mentorminds_security
- **Discord**: Security Team role in official server
- **Twitter DM**: @MentorMinds (for initial contact only)

### Report Template

```markdown
## Vulnerability Report

### Summary

[Brief description]

### Severity Assessment

[Critical/High/Medium/Low with justification]

### Affected Component

- Contract: [escrow/verification/mnt-token/referral]
- Function: [function name]
- Version: [commit hash or tag]

### Description

[Detailed explanation]

### Reproduction Steps

1. Step 1
2. Step 2
3. ...

### Impact

[What can an attacker achieve?]

### Proof of Concept

[Code, transaction hash, or screenshot]

### Suggested Mitigation

[If known]

### References

[Similar vulnerabilities, academic papers, etc.]

### Researcher Info

- Name/Alias:
- Contact:
- Previous disclosures (optional):
```

---

## 🛠️ Security Measures

### Current Protections

1. **Code Quality**:

   - ✅ All contracts pass `cargo clippy -- -D warnings`
   - ✅ Comprehensive unit tests (50+ per contract)
   - ✅ Integration tests with snapshot testing
   - ✅ Fuzzing infrastructure ready

2. **Access Control**:

   - ✅ `require_auth()` on all privileged operations
   - ✅ Admin key stored on hardware wallet (mainnet)
   - ✅ Multi-sig planned for v1.1

3. **Economic Safeguards**:

   - ✅ Fee cap at 10% (1000 bps)
   - ✅ Supply cap on MNT token (100M)
   - ✅ Auto-release prevents permanent lockup

4. **Monitoring**:
   - ✅ Event emission for all state changes
   - ✅ Persistent storage with TTL management
   - ✅ On-chain audit trail

### Planned Enhancements

- 🔜 External security audit (Q2 2026)
- 🔜 Bug bounty platform integration (Immunefi or similar)
- 🔜 Multi-sig admin wallet
- 🔜 Time-lock for parameter changes
- 🔜 Formal verification of critical functions
- 🔜 Insurance fund for residual risks

---

## 📜 Legal Safe Harbor

### No Legal Action

The MentorMinds team will **not** pursue legal action against security researchers who:

1. Follow responsible disclosure guidelines
2. Act in good faith to protect users
3. Do not exploit vulnerabilities for personal gain
4. Cooperate with remediation efforts

### Limitation of Liability

This safe harbor does **not** protect:

- Intentional misconduct or fraud
- Violation of applicable laws
- Breach of confidentiality agreements
- Actions causing demonstrable harm

### Mutual Non-Disclosure

By reporting a vulnerability, you agree to:

- Keep details confidential until coordinated disclosure
- Not share exploit code with third parties
- Allow MentorMinds to lead remediation timeline

We agree to:

- Acknowledge your contribution (unless anonymous preferred)
- Credit you in public disclosures
- Advocate for recognition in the security community

---

## 🏅 Hall of Fame

We recognize security researchers who contribute to platform safety.

### Distinguished Contributors

_To be updated after first responsible disclosures_

### Recognition Levels

🥇 **Critical Discovery**: First to report Critical severity bug  
🥈 **High Impact**: Multiple High severity reports  
🥉 **Community Helper**: Consistent Medium/Low reports  
🎖️ **Special Recognition**: Exceptional contributions

**Privacy Options**:

- Full name and link (default)
- Alias/pseudonym only
- Anonymous (no public credit)

Let us know your preference when submitting reports.

---

## 📚 Additional Resources

### Documentation

- [Threat Model](docs/threat-model.md)
- [Trust Assumptions](docs/trust-assumptions.md)
- [Smart Contract Architecture](README.md)
- [Soroban Security Best Practices](https://soroban.stellar.org/docs)

### Tools & Frameworks

- **Stellar Laboratory**: https://laboratory.stellar.org/
- **Stellar Expert Explorer**: https://stellar.expert/
- **Cargo Fuzz**: https://rust-fuzz.github.io/book/cargo-fuzz.html
- **Kani Verifier**: https://model-checking.github.io/kani/

### Learning Materials

- "Programming Bitcoin" by Jimmy Song
- "Mastering Ethereum" by Andreas Antonopoulos
- SWC Registry: https://swcregistry.io/
- Smart Contract Weakness Classification

---

## 🔄 Policy Updates

This security policy is reviewed quarterly and updated as needed.

### Version History

| Version | Date       | Changes         |
| ------- | ---------- | --------------- |
| 1.0     | 2026-03-25 | Initial release |

### Notification of Changes

Significant changes will be announced via:

- GitHub repository updates
- Discord security channel
- Twitter @MentorMinds
- Email to registered researchers

---

## ❓ FAQ

**Q: Can I test on mainnet?**  
A: **NO.** Only test on local environments or testnet. Mainnet testing is strictly prohibited.

**Q: Do you offer paid audits?**  
A: No. We conduct internal audits and engage independent firms. Unsolicited paid audit offers will be declined.

**Q: Can I publish my findings after X days?**  
A: Only with explicit permission. Default is coordinated disclosure after fix deployment.

**Q: What if I accidentally exploited a bug?**  
A: Return funds immediately and report it. Good-faith accidents are treated leniently.

**Q: Are competitors eligible for bounties?**  
A: Yes, if they follow responsible disclosure. Competitive intelligence is respected.

**Q: Do you accept anonymous reports?**  
A: Yes, though it complicates reward payment and coordination.

**Q: Can I report on behalf of someone else?**  
A: Only with their explicit consent. We prefer direct researcher contact.

---

## 📧 Contact Us

Questions about this policy? Reach out:

**Email**: security@mentorminds.io  
**Subject Line**: "Security Policy Inquiry"

We aim to respond within 24 hours.

---

**Thank you for helping keep MentorMinds secure!** 🙏

Together we can build a safer ecosystem for everyone.
