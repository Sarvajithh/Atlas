import { describe, expect, it } from "vitest";
import { render } from "@testing-library/react";

import { ActivityRail } from "@/components/ActivityRail";

describe("ActivityRail", () => {
  it("renders the activity rail landmark", () => {
    const { getByLabelText } = render(<ActivityRail />);
    expect(getByLabelText("Activity Rail")).toBeTruthy();
  });
});
