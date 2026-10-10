import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { makeProject } from "../../test/fixtures";
import { ProjectProperties } from "./ProjectProperties";

describe("ProjectProperties", () => {
  it("renders the real Project's fields read-only, no edit affordance", () => {
    const project = makeProject({ name: "Orders Project" });
    render(<ProjectProperties project={project} />);

    expect(screen.getByText("Orders Project")).toBeInTheDocument();
    expect(screen.getByText(project.id)).toBeInTheDocument();
    expect(screen.queryByRole("textbox")).not.toBeInTheDocument();
    expect(screen.queryByRole("button")).not.toBeInTheDocument();
  });

  it("shows a clear empty state when no Project is open", () => {
    render(<ProjectProperties project={null} />);
    expect(screen.getByText("No project open.")).toBeInTheDocument();
  });
});
