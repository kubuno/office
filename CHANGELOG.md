# Changelog

All notable changes to **kubuno-office** are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and this
project adheres to [Semantic Versioning](https://semver.org/). Entries are added under
`[Unreleased]` **as the change is made**; `_tools/release.sh` stamps them under the version
number at release time, and CI publishes that section as the GitHub Release notes.

## [Unreleased]

### Changed

- The whiteboard's dockable panels now follow the active theme (light, dark or an
  admin skin) instead of being fixed to a light palette, matching the reworked
  dock chrome shared by the editors.

### Security

- **A deliverable can no longer be accepted behind the owner's back.** Acceptance
  and rejection are the owner's decision and are recorded with who decided and
  when — but an ordinary field update could set the status to "accepted"
  directly, skipping both the permission check and the record. Those two states
  are now reachable only through acceptance or rejection.
- **Whiteboard sessions now check who is connecting.** A whiteboard accepted any
  signed-in visitor into any board without asking whether they had access to it:
  the connection handed over the whole board and let the visitor write to it.
  Access is now verified before the connection is accepted.
- **Sharing levels are now enforced.** "Reader" and "Commenter" were recorded but
  never applied: anyone a project was shared with could edit it, log time on it and
  change its plan. Each action now requires the matching level, deleting a project
  is reserved to its owner, and capturing a baseline stays owner-only.
- **A project can no longer be used as a passkey to another one.** Assigning or
  unassigning a resource, creating a dependency or nesting a task checked the
  project named in the address but acted on identifiers that were never verified
  to belong to it — naming a project of your own alongside someone else's task
  reached straight into their plan. Every identifier is now checked against the
  project it is used with.

### Removed

- **Five unused collaboration endpoints.** Documents, spreadsheets, presentations,
  diagrams and projects each exposed a direct real-time endpoint that nothing has
  called since collaboration moved to the platform's shared service — and each let
  any signed-in visitor into any item's session. They have been removed rather than
  patched. Real-time collaboration is unaffected.

### Added

- **A register of what the project actually spent.** A project could plan its work
  but never record its money. Each work package now carries a **budget** — the
  yardstick its progress is measured against — and the project keeps a register of
  its **direct expenses**: licences, hardware, subcontracting, travel, each dated,
  described and, when it makes sense, charged to a work package. The register opens
  on the total and on how it splits by category, which is the figure people come
  for. A budget left empty means *nobody costed this package* — deliberately not the
  same thing as a package costed at zero, and the two are never conflated. Labour is
  **not** typed in here: it is valued from the hours logged on the tasks times the
  resource's rate, and entering salaries again would count them twice — the screen
  says so where it can be misread, and warns when an expense is filed as labour
  anyway.

- **The currency is now an instance setting.** Budgets, expenses and contracts
  were kept in euros because that is what the code said. The administration
  console now carries a default currency for the module (Modules → Office →
  Projects → Costs), offered as a list of the usual codes — the CFA francs and
  the Maghreb currencies included, not only the ones a European default assumes.
  A new project takes it; **existing projects keep theirs**, because a change of
  policy must not silently restate amounts already entered.
- **A portfolio view across projects.** Every project the user can see, judged by
  the same measures and put side by side: progress, work already late, high risks
  still open, overdue issues, changes waiting for a decision, deliverables
  accepted, and what is committed on contracts where the project — not the
  supplier — absorbs an overrun. What needs attention is named rather than scored,
  because a single health figure hides which of the six things is wrong. Nothing
  is stored: it reads the registers the projects already keep, in queries grouped
  across the whole set, so the view does not slow down as the portfolio grows. It
  sits beside the existing portfolio tree, under a "Santé" tab: the tree says how
  the projects are arranged, the board says which of them needs you.
- **A contract register that says who carries the risk.** A project can now record
  what it buys rather than builds: the statement of work, the supplier, the terms,
  the payment schedule — and, above all, the **contract type**, which is the field
  almost no register keeps and the one that decides who pays when the estimate
  turns out to be wrong. A fixed price puts an overrun on the supplier; a
  cost-reimbursable contract puts it straight back on the project. The committed
  total is therefore never shown as a single figure: it is split by who absorbs
  the overrun, because ninety-nine thousand committed of which forty-eight sit
  with the supplier is not the same exposure as ninety-nine thousand in day rates.
  Time-and-material contracts with no ceiling are called out on their own — that
  is not an amount, it is the absence of one, and nothing bounds what such a
  contract can cost.
- **Contracts left open now block the closure.** A contract outlives the project
  that signed it, so the closure checklist counts the ones still awarded or
  running among the things left undone. The register also refuses to award a
  contract with no committed value — it would commit something nobody can add up —
  and refuses to delete one that was awarded: the register keeps what was
  committed, and a contract that came to nothing is cancelled, not erased.
- **A project can now be closed, and closing it means confronting it.** Almost
  everything a closure report should say was already recorded — which deliverables
  were accepted, which changes were decided, which issues are still open, which
  requirements were never verified. What was missing was the act of putting the
  project in front of all of it before declaring it over. A closure checklist now
  runs against every register the project keeps and separates what is left
  **undone** from what is left **unwritten**. A project can still be closed with
  points open; it cannot be closed *quietly* — the reason is asked for, named
  against what remains, and kept with the closure. And it cannot be closed at all
  until someone answers the one question the charter asked: were the objectives
  met?
- **Lessons learned, with the part that travels.** Each lesson records the
  situation, what happened, and the **recommendation** — the only part any use to
  the next project. A lesson cannot be validated without one: without it, it is an
  anecdote that leaves the next project to work things out for itself. Lessons are
  recorded as positive, negative or mixed, because a register that holds only
  failures teaches people to hide them, and each can be traced back to the task,
  risk, issue or change that produced it.
- **Change control, with the rule that makes it worth having.** Anything asked for
  after the plan was agreed is now recorded as a change request: what is wanted,
  why, and how urgently. It is then **assessed** — how many days, how much money,
  and what it does to the scope, the risks and the quality — and only then decided
  on, by the project's owner. **A change nobody has assessed cannot be approved.**
  That refusal is the whole point: approving something whose cost nobody worked
  out is exactly what change control exists to prevent. A rejection has to say
  why, and a partial approval has to say what was kept. Once decided, a request
  can no longer be edited or deleted — the register keeps what the board actually
  read; a request that no longer applies is withdrawn, not erased.
- **What the approved changes have already done to the plan.** The register adds
  up the assessed impact of everything that was said yes to and reports it as one
  figure — so many days and so much money added since the baseline. It is the
  number that explains why a project no longer matches the plan it is being judged
  against, and nobody usually keeps it. It also points out approved changes for
  which no baseline was ever named: the change is in the plan, but nothing records
  what the plan became.
- **A communication plan that can be audited.** A project can now state who is
  told what, through which channel, in what form and how often — and, because the
  plan is crossed with the stakeholder register, it answers the question actually
  worth asking: **who receives nothing**. A regulator with power over the project
  and no line in the plan is the same defect as a requirement nothing realises,
  and the uncovered are listed weightiest first. Recording that something went out
  moves the next date forward by the communication's own frequency, so a report
  sent stops being counted as late; for a milestone or on-demand note, which has
  no next date to compute, the view says so rather than leaving a stale one.
- **A decision log that keeps the reasoning, not just the conclusion.** A project
  makes choices that outlive the reason for them. Each decision now records the
  question that had to be settled, what was chosen, why, **what was ruled out** —
  the half everyone forgets and the one worth the most six months later — and what
  it commits the project to. A decision can replace an earlier one, which is then
  marked superseded and reads as history rather than as a mistake, with each end
  of the chain naming the other. The log leads with its own defect: decisions
  recorded as taken with no reasoning behind them, which nobody will be able to
  revisit.
- **Quality, in numbers rather than in opinions.** A project could state what
  "good" meant in prose and never check it. It now keeps quality metrics — each
  with the way the number is obtained, a target, and the band that still counts as
  conforming — measured again and again rather than once. Three answers are kept
  apart on purpose: conforming, out of tolerance, and **not assessable** — a metric
  with no target is a number being collected, not a standard being held to, and
  calling it conforming would be an invention. A fourth case is called out
  separately: still inside tolerance, but close to the edge and heading that way.
  That one is invisible in a single reading and is usually the one worth acting on.
- **Checks that leave evidence.** A deliverable could be declared accepted with
  nothing to show for it. Quality checks now record, against a deliverable or a
  work package, what was verified and what it found; a check must qualify
  something, and one cannot be marked failed without saying what it turned up.
- **Cost of quality.** Expenses can be classified as prevention, appraisal,
  internal failure or external failure, and the view puts the two sides face to
  face: what the project spends to avoid failure against what it pays when that
  did not work. Nothing is entered twice — the classification hangs on the
  expenses already recorded — and what nobody classified is reported as such
  rather than folded into one of the four.
- **A stakeholder register, and a RACI beside it.** A project knew its tasks but
  not the people who could help or block it. The register records, for each one,
  how much power they hold and how much interest they take — the two axes of the
  usual grid, which sorts them into those to manage closely, keep satisfied, keep
  informed or merely watch. It also records how engaged they are *today* against
  how engaged the project *needs* them to be, on the five standard levels from
  unaware to leading. The gap between the two is the only actionable part: a
  register that notes only the present state describes a situation instead of
  asking for anything, so the view leads with who is furthest from where they need
  to be, weighted by how much they matter.
- **Who answers for each task.** A RACI matrix says, task by task, who does the
  work, who answers for it, who is consulted beforehand and who is told afterwards.
  One rule is enforced rather than suggested: **exactly one person answers for a
  task** — two accountable people is nobody accountable. Naming a second is refused
  by name, and the database itself forbids it. The matrix leads with what it is
  missing, which is why it is worth drawing: the tasks nobody answers for, and the
  tasks nobody is doing. Summary tasks are left out — their children already carry
  the roles.
- **Earned value: where the project will land, not just what it has spent.**
  From the budgets, the rates and the expenses above, the module computes **earned value**: what was planned to be done by a
  chosen status date, what has actually been done *valued at its budget*, and what it
  cost. The distinction matters because spend against budget says nothing on its own —
  a project that has burnt half its money may have done a tenth of the work. It reports
  the cost and schedule variances, the two performance indices, and a forecast of the
  final cost under whichever of the four standard assumptions the project stands behind,
  along with what performance the remaining work would have to achieve to still land on
  budget. A curve draws the three lines against time.
- **The measurement says when it cannot be trusted.** Indices computed over a plan
  that is half unbudgeted, or against hours nobody logged, read far better than the
  project is doing — a project with no timesheets shows a magnificent cost index
  because its labour never appears. The view now states, before any figure, how many
  packages are budgeted, how many hours are recorded, and how much of the cost is
  labour versus direct spend.
- **A risk register, and an issue log beside it.** A project had nowhere to write
  down what might still go wrong. It now keeps a register: each risk with its
  category, the early signs that would announce it, who watches it, and the
  response chosen — and the response offered depends on what kind of risk it is,
  because you do not mitigate an opportunity, you enhance it. **Opportunities are
  managed alongside threats**: a register that only records bad news teaches
  people to look one way. Risks are ranked by probability × impact on the usual
  five-by-five scale, and the register draws the probability/impact matrix, so
  exposure can be read as a shape rather than a list. Where a chance and an
  amount are known, the expected monetary value is computed — negative for a
  threat, positive for an opportunity — and summed into the provision the project
  ought to be holding.
- **A risk that comes true becomes an issue in one action.** The register is
  about the future; the issue log is about now, and keeping them apart is what
  stops a forecast from silently turning into a list of things that have already
  happened. Marking a risk as occurred opens the matching issue, carries over its
  description and severity, and keeps the link between the two — pressed twice, it
  does not open a second one. An issue cannot be closed without saying how it was
  resolved, and the log leads with the count that actually demands something: how
  many are open and past their date.
- **The plan is now numbered.** Every task has carried an empty "WBS" code since
  the beginning — the column existed, nothing ever filled it. Each element of the
  breakdown now has its outline number (1, 1.2, 1.2.3), derived from the tree
  itself and recomputed whenever the plan is reshaped, so a task moved under
  another takes its new address and its whole sub-branch with it. A **WBS
  dictionary** goes with it: for each work package, its statement of work,
  acceptance criteria, assumptions, quality requirements, risks, who is
  accountable — and, deliberately given its own place, **what the package
  excludes**. The line a work package is measured against when someone asks for
  one more thing.
- **Deliverables are followed through to acceptance.** A project could show every
  task green while nothing had actually been handed over and agreed. It now keeps
  the list of what it delivers — with a code, the work package that produces it,
  a due date and acceptance criteria — and each one is accepted or rejected by
  the project's owner, a rejection stating why. The count of accepted deliverables
  is the one measure of progress that cannot be talked up.
- **Requirements and their traceability matrix.** Requirements are recorded with
  their type, their MoSCoW priority, where they come from and how they will be
  verified — then traced to the deliverable that satisfies them and the work
  package that builds it. The matrix answers the two questions it exists for:
  which requirements nothing realises, and which deliverables no requirement
  justifies. A promise nobody is keeping, and work nobody asked for.
- **Projects now begin with a charter.** A project went straight to a task list,
  with nothing recording why it exists, who authorised it or what success would
  mean — the questions a plan is supposed to answer before its first task. Each
  management project now has a charter: its purpose and business case, objectives
  and success criteria, high-level requirements, assumptions and constraints,
  major risks, budget summary, sponsor, project manager and the authority granted
  to them. It can be **approved** by the project's owner, after which it becomes
  read-only: changing an approved charter means reopening it with a stated reason,
  which files the approved version as a dated revision rather than overwriting it.
  The charter also carries the high-level milestones the project commits to, and
  one action turns them into real milestones in the schedule — run it again after
  a change and it updates what it created instead of duplicating it. Like every
  other artifact, a project that does not want a charter can switch it off.
- **A project can keep several baselines and say which one counts.** Baselines were
  a flat list you picked from each time; one of them is now *the* reference the
  project is judged against — the first you capture, and any other you promote —
  and they can be renamed. A new comparison reports, task by task, how far the plan
  has drifted from what it promised, including the tasks that have since been
  removed rather than quietly dropping them.
- **Public holidays can be imported into a project's calendar.** One action fills
  the working calendar with the holidays of the country the instance serves, as
  non-working days you can still reopen one by one — a plan that works through a
  holiday says so explicitly.
- **The schedule now knows which days are worked.** A five-day task simply spanned
  five calendar days, weekend included, so every date past the first week was wrong
  for anyone working Monday to Friday. A project now has a working calendar — the
  days of the week it works, plus the days that depart from it: a public holiday
  that closes an open day, a catch-up Saturday that opens a closed one. Durations
  are counted in worked days, and changing the calendar marks the plan for
  recalculation.
- **Tasks can now be pinned to dates.** A task could only be pushed by the ones
  before it; a plan also has fixed points — work that cannot start before a permit,
  a milestone owed on a contractual date. A task can now be told not to start
  before, not to finish after, or to fall exactly on a given date, using the eight
  standard constraint types. When a fixed date cannot be met the task shows
  negative float rather than a schedule quietly pretending to fit.
- **Tasks can carry a deadline.** Separate from a constraint: a deadline never
  moves the schedule, it only reports that the task is finishing late.
- **A project now shows only what it uses.** Every project offered every view —
  schedule, board, calendar, workload, network, roadmap — plus baselines and the
  time log, burying a three-task plan under an interface built for a construction
  programme. A new settings panel lets a project state how it is run (predictive,
  agile or hybrid), which decides the view it opens on, and switch off the
  artifacts it does not need. Each change is saved as you make it, and the last
  remaining view cannot be switched off.
- **Projects shared with you are now listed.** A project someone shared could not
  be found anywhere — you had to be given a direct link. The projects list can now
  show them, and opening a shared project's file from Drive works too.
- **"Shared with me" view.** A new sidebar entry (and mobile tab) between Favourites
  and Trash opens the list of projects other people shared with you. It only lists
  what was shared — creating a project from there is not offered — and says so
  plainly when nothing has been shared with you yet.
- **Time log on tasks.** Each task in a management project gains a time log: dated
  entries with an activity (development, design, coordination, testing,
  documentation, other) and an optional comment, added and listed from the task
  inspector. The sum of a task's entries is rolled up into its "spent hours" (kept
  separate from the estimate), so the spent field now reflects logged time rather
  than a manual number.
- **Roadmap and release versions.** Management projects gain a "Roadmap" view (in the
  Affichage ribbon tab) that groups tasks into named release versions. Each version
  shows a progress bar and completed/open task counts, an editable name, an
  open/locked/closed status, and an optional due date; its related tasks are listed
  with completed ones struck through. Versions can be created, renamed and deleted,
  and a task can be assigned to a version from the task inspector. Finished versions
  are hidden behind a toggle.
- **Work tracking on tasks.** Each task now carries an estimated effort and time
  spent (hours), editable in the task inspector; the project's backstage shows the
  estimated and spent totals across all tasks.
- **Portfolio tree.** The Projects home gains a "Portefeuille" tab listing every
  project as an expandable tree — subprojects nested under their parent, with type,
  status and last-modified columns and a search. Click a row to open the project.
- **Project hierarchy.** A project can now be a subproject of another (a project is
  the "folder" of its children — one optional parent, any depth). The cloud project
  dashboard gains a Hierarchy card to set the parent and list subprojects; cycles and
  self-parenting are refused. Deleting a parent detaches its children to the root
  rather than deleting them.
- **Baselines for management projects.** Save a named snapshot of the planned
  schedule ("Définir" in the ribbon), then compare it to the current plan: the
  Gantt draws a thin ghost bar under each task at its planned position (red when
  the task has slipped later, green when earlier), a new **Écart** column shows the
  start variance in days, and a legend names the baseline being compared. Baselines
  can be listed, switched and deleted; the comparison is exact because it stores the
  computed schedule offsets, not a replayed history.
- **Projects now start from a type.** Creating a project opens a picker instead of
  dropping straight into an empty plan: **Management project** (the existing
  Gantt/critical-path planner) or **Cloud project** — a container of resources with
  an immutable, instance-unique identifier derived from its name (editable before
  creation, fixed after), and key/value labels.
- **Cloud projects use the same editor shell as management projects** — the ribbon
  (File / Home / View tabs), an editable title and the backstage — with their own
  pages switched from the View tab: a **dashboard** (project info, access, labels,
  resources cards), an **Access** page to grant people a role (viewer / commenter /
  editor) with a person search, and a **Labels** editor.
- **A cloud project's Resources are the Kubuno modules it uses.** The Resources page
  lists every installed module and lets the owner attach or detach any of them, so a
  project groups the modules it relies on — the sovereign counterpart of a cloud
  console's enabled services. The Consumption page explains that usage is tracked
  per module and account today, not yet per project.

### Changed

- **Documents has its own logo** — a blue shield carrying a page — instead of the
  generic document glyph it shared with everything else. It appears in the apps
  grid and on the browser tab while in Documents.
- Theme tokens: two colours for navigation labels (`--color-text-nav`,
  `--color-text-nav-active`). Every module carries the same token sheet, so the
  values must match across them — whichever bundle loads last would otherwise
  win. No visible change inside this module.
- Default application background token aligned with the core (`--body-bg` `#f8fafd`). Only
  visible when the module runs standalone: inside the shell the active theme sets it.

[Unreleased]: https://github.com/kubuno/office/compare/v0.1.5...HEAD
- **Listings now say which Drive file each item is backed by.** Documents, reports
  and whiteboards report their file (and, for a document, the file it was imported
  from) alongside the rest of their attributes, so maintenance can tell which files
  still belong to something instead of guessing.
- **Consistent form fields across the projects module.** Every field now uses the
  core UI primitives with matching height and font size on a given row — search
  boxes, date fields, the role and version selectors, and the task inspector — so
  neighbouring fields line up instead of mixing sizes.

### Fixed

- **Dates no longer show the day before.** A date without a time was read as UTC
  midnight, so anywhere west of Greenwich a due date displayed one day early.
- **Cards no longer stay white in the dark theme.** The project views painted
  their cards with a hard-coded white instead of the theme's surface colour, so
  they ignored the theme entirely.
- **Dictionary fields now grow to their text.** The last line of a long entry —
  typically what a work package *excludes*, the one read under pressure — was cut
  off behind a scrollbar.

- **A link that would create a loop is now refused.** Nothing stopped a chain of
  tasks from pointing back at itself, and the schedule then quietly gave up on the
  tasks caught in the loop, leaving them with default dates as if all were well.
  Such a link is now rejected as you draw it, naming why — and if a plan already
  contains one, the schedule says so instead of publishing dates it could not work out.
- **Switched-off artifacts no longer linger in the task inspector.** Turning off the
  time log or the roadmap removed them from the ribbon but left the time entries and
  the version selector sitting in the inspector.
- **Task links now mean what they say.** A link between two tasks could be set to
  start-to-start, finish-to-finish or start-to-finish, and given a lead or a lag in
  days — but the schedule ignored all of it and treated every link as a plain
  finish-to-start with no delay. Planned dates were therefore wrong wherever a plan
  used anything else. All four relationship types and signed lags are now honoured
  in both directions, so the critical path reflects the plan actually drawn.
- **Tasks now show their free float.** Alongside the existing float — how long a
  task can slip before the project finishes late — the inspector shows how long it
  can slip before it pushes the task that follows it. The two answer different
  questions, and a task can have plenty of one and none of the other.
- **Duplicating a project now copies the project.** It used to create an empty
  shell: only the project's own details were copied, and its tasks, subtasks,
  links, resources, assignments and versions were left behind. The copy now
  carries all of them, with every internal reference rebuilt so nothing points
  back at the original. Time entries, baselines and the people it was shared with
  are deliberately left out — hours record work someone actually did, a baseline
  is a commitment made on the original, and a copy starts private.
- **Deleting a task that does not exist no longer reports success.** The request
  answered "done" whether or not it had removed anything; it now says so.
- **Requests that changed nothing no longer report success.** Trashing, restoring
  or permanently deleting a project answered "done" even when it did nothing; they
  now say what went wrong — and permanent deletion states that the project must be
  in the trash first.
- **Deleting a document, spreadsheet, presentation, diagram or project now removes
  its file from Drive.** Permanent deletion used to drop only the database record
  and leave the file behind, so Drive kept listing files that could no longer be
  opened — clicking one answered "not found". Files you imported yourself
  (the original .docx/.xlsx of a converted document) are still kept.
- **Two projects with the same name no longer share (or destroy) one file.**
  Creating a project overwrote any Drive file that happened to carry the same
  name, so an earlier project silently lost its file and vanished from Drive.
  Each project now gets its own numbered file, and projects left without one —
  or sharing one — repair themselves on the next save.
- **Recent documents no longer list documents that were deleted.** The Documents
  start page kept its recent list in the browser, so deleted documents stayed on
  it and led nowhere when clicked; it now reflects what the server actually holds,
  like every other module.
- **Opening a file from the Browse tab now finds the right document.** Documents,
  spreadsheets, presentations, diagrams and projects were opened by following an
  identifier stored on the file, which could name an item that no longer existed —
  the file then opened onto nothing even though it was perfectly valid. The link
  recorded on the item itself is now used instead, with the stored identifier kept
  only as a fallback.
- **A project that cannot be opened now says so.** Opening a project that no longer
  exists — or that you have no access to — used to hang on "Loading…" forever; it
  now shows a clear "Project not found" message with a way back to your projects.
- **Opening a shared project no longer fails with "not found".** A project shared
  with you through the access panel could not be opened (the editor returned a 404);
  a project is now readable by its owner *and* its collaborators.
- The Projects editor no longer claims the generic `.json` extension and
  `application/json` type. It used to open **any** JSON file the user stored as a
  project (and mis-render it); only the Kubuno project format (`.kbprj`) opens in
  it now.
