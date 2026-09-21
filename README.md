# My Daily Newspaper (Windows version)

A newspaper with exactly one subscriber: you. Every morning Claude researches the things you care about, writes them up as real newspaper stories, and lays out a broadsheet front page with your name on the masthead. Optionally it lands on your printer before you're out of bed.

On a Mac? Use the Mac version instead: `my-daily-newspaper`.

> ## You need these before it will run
>
> | | What | Why |
> |---|---|---|
> | **Required** | **Windows 10 (1809 or newer) or Windows 11**, 64-bit | This is the Windows version. |
> | **Required** | **[Claude Code](https://claude.com/claude-code)** installed and **signed in with your own Claude subscription**. In PowerShell: `irm https://claude.ai/install.ps1 \| iex`, then run `claude` and `/login` | Claude is the editor. The app runs the `claude` command that's already on your PC. No Claude login, no newspaper. Claude Code needs a paid Claude plan (Pro, Max, Team or Enterprise). |
> | Builder offers to install | **Microsoft C++ Build Tools**, **Rust**, **Node.js 20+**, **WebView2 runtime** | Needed to compile and show the app. The builder checks for each and, with your OK, installs what's missing through `winget`. The C++ tools are a several-GB one-time download. |
> | Optional | **Grok CLI** signed in with a **SuperGrok** subscription. In PowerShell: `irm https://x.ai/cli/install.ps1 \| iex`, then `grok login` | Adds "The X Wire" section: real posts from X. Without it the paper still prints, minus that section. |
> | Optional | **[SumatraPDF](https://www.sumatrapdfreader.org)** (free): `winget install SumatraPDF.SumatraPDF` | **Only for automatic printing.** It's what sends the paper edition to your printer with nobody at the keyboard. The builder offers to install it. |
>
> Microsoft Edge makes the PDF for the paper edition. It ships with Windows, so there is nothing to install for that. (Chrome or Brave work too.)
>
> **No accounts, keys or logins ship with this project.** There are no API keys anywhere in it. Each person's copy uses the Claude (and Grok) login that lives on *their* PC, under their own user folder, and spends *their* plan's usage. Nothing of the author's is shared, and nothing of yours leaves your machine except the research Claude and Grok do on your behalf.

## Install

1. Get the code: `git clone <this repo>` (or download the zip and unpack it somewhere without odd characters in the path, like `C:\Users\you\my-daily-newspaper-windows`).
2. Double-click **`Build My Daily Newspaper.bat`** in the project folder.
   If Windows shows "Windows protected your PC", click **More info**, then **Run anyway**. (That warning appears for any script that came from a zip download.)
3. It checks the build tools and offers to install whatever is missing. Say `y` to each; Windows may ask for permission once or twice.
4. It asks your **first name**. That goes on the masthead ("Sam's Daily") and your initial goes on the app icon (SD). Press Enter to skip; you can set it later in the app.
5. 5 to 10 minutes later **My Daily Newspaper** is installed for your user (no admin needed), has Start Menu and Desktop shortcuts, and opens itself. You never need the builder again unless you want to update.

Why does everyone build their own copy instead of downloading an installer? Because an unsigned installer from the internet trips SmartScreen and antivirus on other people's PCs. A program you compiled yourself doesn't.

Everything the builder prints is saved to `build.log` in the project folder. Your name is remembered in `.owner` (never committed); delete that file to be asked again, or pass a name: `"Build My Daily Newspaper.bat" Priya`.

Where things end up:

| What | Where |
|---|---|
| The app | `%LOCALAPPDATA%\Programs\My Daily Newspaper\My Daily Newspaper.exe` |
| Your paper (interests, editions, settings) | `%APPDATA%\com.mydailynewspaper.app\` |
| Shortcuts | Start Menu and Desktop |

To uninstall: turn **Deliver daily** off in the app (that removes the scheduled task), then delete the two folders and the two shortcuts above.

By hand, if you prefer:

```powershell
npm install
npm run tauri dev                                   # live-reload development
$env:DAILY_OWNER_NAME = "Sam"; npm run tauri build -- --no-bundle
```

## First launch

A welcome page asks three things: your first name, a city for the dateline, and a ball team for the score box (or "I'm lame and don't like baseball", which removes it). Then you land on **Edit Interests**, pre-filled with a starter set of beats: people, topics, YouTube channels. Change them, switch some off, add your own, or hit **Skip for now** and do it later. **Save & print my first edition** starts the presses. The first edition takes 2 to 4 minutes.

## What's on the page

- **The front page.** Text-forward, like a paper: headline, standfirst, byline, then a story Claude actually wrote, so you can read the whole paper without clicking anything. Each story ends in **Read more...**, **See the video...** or **See the post...**; an article about a video gets both buttons. Columns are filled: the editor writes to a character budget per story size (lead 1,500-2,300 characters, features 900-1,350, briefs 520-800, posts 240-420), a copy-desk pass sends back anything that came in short, and the layout drops each story into the shortest column so nothing ends in a hole.
- **Refresh** (top right) prints a new edition on demand. A new one also prints itself on the first launch of each day.
- **Press Room** (top right box) narrates the run live: what Claude is searching, a progress bar with a percentage, total time so far, and a rough time left.
- **Morning delivery** (top left box): pick an hour and the edition is researched in the background every day, with no window, so it's waiting when you open the app. Tick **and on paper** and it goes to your printer too.
- **Print** (top right) lays the edition out for paper and opens the PDF. Print it from there.
- **The score box** (top center): your MLB team, straight from MLB's public stats API, no AI involved. During a game: score, inning, who's on base, balls, strikes, outs, and ABS challenges left when the league reports them. Otherwise: the last final, plus the next game as "Dodgers vs. Padres - Today 5:10 PM PDT - SportsNet LA". It's on the printed paper too. Any of the 30 teams, or none.
- **Edit Interests**: add, remove, edit, reorder or switch off beats. Your name, city and team live at the top of that page.
- **The reader window.** Click any button, headline or thumbnail and the real page opens in a floating window with a newspaper frame and three buttons: **Ad block - Full screen - Close** (Esc closes too).

## Morning delivery

The switch creates a task in Windows Task Scheduler called **My Daily Newspaper - Morning Delivery** that runs this same app with `--refresh-only`. Turning the switch off deletes the task. You can see it in the Task Scheduler app, but you never need to.

- **PC asleep or off at that hour?** The task is set to run as soon as possible after a missed start, so the paper is made when the PC is next on and you're signed in. The run waits up to 3 minutes for the network. It does not wake the PC by itself.
- It runs as you, only while you're signed in, with no admin rights and no stored password. It also runs on battery.
- An edition less than 3 hours old is left alone, so a catch-up run right after a manual Refresh doesn't burn a second one.
- The background run and the open window are separate copies of the app. A lock file keeps them from printing at once, and the open window checks the disk every 5 minutes (and whenever it regains focus) and swaps in a newer edition by itself.
- Log: `background.log` in the data folder.
- The task points at the installed app. Rebuilding keeps it working, because the builder installs to the same place every time.

## The paper edition

Printing turns the edition into a proper three-column Letter-size newspaper: masthead, scoreboard, photos in black and white, and a small QR code per story, since paper can't be clicked. Links are named, never spelled out ("Read more on techcrunch.com", "See the video on YouTube"). It prints double-sided and never more than 8 pages.

- **The PDF** is made by Microsoft Edge running invisibly for a few seconds. It never opens a browser window and never touches your browser profile.
- **Automatic printing** ("and on paper") hands that PDF to **SumatraPDF**, which prints it silently. Without SumatraPDF the switch tells you what's missing, and the Print button still opens the PDF for you to print by hand.
- Uses your Windows default printer, as long as it's a real one. "Microsoft Print to PDF", XPS, OneNote and Fax don't count: sending a job to those opens a Save dialog nobody is there to answer. If your default is one of those and you have exactly one real printer, that one is used; otherwise name one in settings.
- Automatic printing happens once a day at most, and only as part of the scheduled morning run. If the run happens more than 6 hours late, it skips the paper rather than printing yesterday's news at dinner time.
- PDFs are kept for 14 days in the data folder under `print\`.

## How an edition gets made

| Step | Who | What | Time |
|---|---|---|---|
| 1. Wire | Rust | YouTube channel feeds and news search feeds for every enabled interest. Links are real by construction. | seconds |
| 2. X wire | `grok -p` | Grok CLI searches X for posts you'd like. Runs alongside step 1, on your SuperGrok login. Optional. | ~1 min |
| 3. Editor | `claude -p` | Gets your interests plus the wire copy, researches in two parallel rounds, picks the stories and writes each one to length. Sonnet at low effort by default: fast and light on your plan. | 1.5-3 min |
| 3b. Copy desk | `claude -p --resume` | Only if stories came in under length: the same session is asked to fill them out. | 0-40 s |
| 4. Pictures | Rust | YouTube thumbnails and share images. | seconds |
| 5. Press | React | JSON becomes the front page, saved as `editions\YYYY-MM-DD.json`. | instant |

Each refresh is a real research run against your Claude plan's usage. Claude is only given web search and web fetch. It can't run commands or touch files, and it runs in an empty scratch folder.

## Your data and settings

Everything lives in `%APPDATA%\com.mydailynewspaper.app\` (paste that into File Explorer's address bar). Nothing is stored in the project folder, so `git pull` and rebuilding never touch your paper.

- `interests.json` - what Edit Interests edits
- `editions\` - one JSON per day
- `print\` - recent PDFs
- `channel-cache.json` - YouTube handle to channel id
- `background.log` - what the scheduled runs did
- `settings.json` - knobs. Missing keys use their defaults. In paths, use forward slashes or double the backslashes: `"C:/Tools/claude.exe"`.

| Key | Default | Meaning |
|---|---|---|
| `ownerName`, `city`, `mlbTeamId` | builder's name, Los Angeles, 119 | The welcome page values. `mlbTeamId` 0 hides the score box. |
| `claudeModel` | *(empty = sonnet)* | `opus`, or `default` for whatever your CLI is set to |
| `claudeEffort` | `low` | `medium`, `high`, or `default` |
| `maxCards` | 30 | Upper bound on stories per edition |
| `useGrok` | true | Use the Grok CLI for the X wire when it's installed |
| `blockAds` | true | Ad blocking in reader windows |
| `readerAlwaysOnTop` | true | Reader floats above other windows |
| `printDaily` | false | The "and on paper" switch |
| `printer` | *(empty = Windows default)* | The printer's name exactly as Settings > Printers & scanners shows it |
| `printColor` | false | Colour photos on paper |
| `printQr` | true | QR code per story |
| `printDuplex` | true | Double-sided |
| `printMaxPages` | 8 | Never send more than this. 0 = no cap |
| `chromeBin` | *(empty = auto-detect)* | Path to the browser that makes the PDF (Edge, Chrome, Brave) |
| `sumatraBin` | *(empty = auto-detect)* | Path to `SumatraPDF.exe` |
| `claudeBin`, `grokBin` | *(empty = auto-detect)* | Full paths, if the app can't find them. `where.exe claude` in PowerShell shows yours. |
| `claudeTimeoutSecs`, `grokTimeoutSecs` | 600, 240 | Hard stops |

## The reader window and ad blocking

The reader is a real native window on the page itself (not an iframe, so sites can't refuse to load). **Ad blocking** works in two layers off one short list of ad-tech domains (`src-tauri/src/adblock.rs`): Rust cancels ad redirects at the navigation level, and an injected script stops ad scripts before they run and hides the usual ad containers. The counter on the button shows what it caught. If a site refuses to load with a blocker, click **Ad block on** to switch it off for that window. It never runs on YouTube or X: their ads are first-party, and YouTube stops playing when it thinks it's being blocked.

Security: reader windows load other people's pages, so they get zero access to the app. `capabilities/default.json` grants remote pages nothing; a probe page that tried `get_interests`, `refresh_edition`, `open_reader`, `window.close` and `event.listen` was denied every one. The reader's own buttons don't use the app's API at all: they navigate to a fake address that Rust intercepts. Pages can't steer the reader into `tauri://`, `asset://`, `ipc://` or `file://`, and plain `http://` links are upgraded to `https://`.

## Where things live

```
Build My Daily Newspaper.bat       the double-click builder (starts tools\build.ps1)
tools/build.ps1                    checks tools, asks your name, builds, installs
tools/icon-maker/                  draws the initials icon (PNG sizes, .ico, .icns)
src/                               React front page
  App.tsx                          shell: masthead, refresh, print, routing
  components/FrontPage             column-filling layout: lead, flanking stories, desks
  components/StoryCard             article / video / post cards
  components/PressRoom             live status, percentage, elapsed time
  components/ScoreBug              the score box
  components/Delivery              Morning delivery + "and on paper"
  components/Welcome               first-run page
  components/InterestsPage         your paper + your beats
  mock.ts                          sample copy for browser-only dev
src-tauri/src/
  lib.rs                           commands + the refresh pipeline
  editor.rs                        Claude CLI runner, prompt, length budgets, copy desk
  grok.rs                          Grok CLI runner + post parsing
  wire.rs                          feeds, channel-handle lookup, share images
  print.rs                         paper edition: HTML, PDF (Edge), printer (SumatraPDF)
  scores.rs                        MLB score box
  schedule.rs                      Morning delivery (Windows Task Scheduler)
  reader.rs, reader_inject.js      the reader window and its frame
  adblock.rs                       ad-tech domain list + blocking rules
  paths.rs                         finds claude / grok; starts helpers without console windows
  store.rs                         everything on disk, the refresh lock, profile
src-tauri/build.rs                 puts your initials icon into the .exe and the window
src-tauri/resources/default-interests.json   the starter beats new readers see
```

UI only, no backend, sample copy (handy for restyling): `npm run dev`, open http://localhost:1420. Add `?live=1` for a live game in the score box, `?welcome=1` for the first-run pages, `?empty=1` for the no-edition state.

## Tests

```powershell
cd src-tauri
cargo test --lib                                         # unit tests, no network
cargo test --lib e2e_claude -- --ignored --nocapture     # a real Claude run, ~3 min, uses your plan
```

## How this relates to the Mac version

Same paper, same front end, same editor. Three pieces are different because the operating systems are: the daily schedule (Task Scheduler instead of a LaunchAgent), printing (Edge + SumatraPDF instead of Chrome + `lp`), and the builder (`.bat` + PowerShell instead of a `.command` script). The Rust code keeps both paths side by side and picks at run time, so almost all of the Windows logic is compiled and unit-tested on every platform.

## Known limits

- First release of the Windows port. The platform pieces are unit-tested, but the full build was written without a Windows machine in the loop; if the builder stops, `build.log` has the details.
- MLB only in the score box.
- The navigation-level half of ad blocking covers page redirects; on Windows, ad frames inside a page are handled by the injected script layer only.
- If your Claude CLI is old enough to reject one of the optional flags, the app retries with the bare minimum. Same for Grok. If Grok is missing or signed out, the edition still prints and a note at the bottom says why.
- A YouTube handle that doesn't resolve shows up as a note at the bottom of the edition. Fix it in Edit Interests.
- Video stories are written from the description and coverage Claude found, not from watching the video. The prompt forbids invented quotes.

## Credits

Masthead font: [Chomsky](https://github.com/ctrlcctrlv/chomsky) by Fredrick Brennan, SIL Open Font License (license in `src/assets/fonts/` and `tools/icon-maker/`). Built with [Tauri](https://tauri.app). Not affiliated with any newspaper, with Anthropic, or with xAI.
