#!/usr/bin/env python3
"""Builds the standalone HTML replicas of the desktop app screenshots in ../real-app/.

Each page is static markup that uses the desktop app's own stylesheet (styles.css is
desktop/src/index.css plus a few lines for stills) and the app's own class names, so it
renders like the running app: same tokens, fonts, spacing and icons. Nothing is scripted
except scrolling a long conversation to its end.

    python3 build.py          # writes NN-name.html and index.html here
"""
import html
import os

from lucide_icons import ICONS

HERE = os.path.dirname(os.path.abspath(__file__))
esc = html.escape


# ---------------------------------------------------------------- icons

def svg(name, size=15, sw=1.75, fill=None):
    f = f' fill="{fill}"' if fill else ' fill="none"'
    return (f'<svg xmlns="http://www.w3.org/2000/svg" width="{size}" height="{size}" viewBox="0 0 24 24"{f} '
            f'stroke="currentColor" stroke-width="{sw}" stroke-linecap="round" stroke-linejoin="round" '
            f'aria-hidden="true">{ICONS[name]}</svg>')


class _I(dict):
    __getattr__ = dict.__getitem__


# The same names and sizes as desktop/src/ui.tsx `I`.
I = _I(
    forward=svg('arrow-right'), globe=svg('globe'), home=svg('house', 13), code=svg('code'),
    external=svg('external-link'), camera=svg('camera', 14), quote=svg('quote', 14), link=svg('link-2', 14),
    dockBottom=svg('panel-bottom'), dockSide=svg('panel-right'),
    up=svg('chevron-up', 13), filter=svg('filter', 13),
    back=svg('arrow-left'), refresh=svg('rotate-cw'), plus=svg('plus'), gear=svg('settings'),
    chev=svg('chevron-right'), down=svg('chevron-down', 13),
    branch=svg('git-branch'), tool=svg('wrench', 13), sub=svg('corner-down-right', 13), tree=svg('network', 13),
    send=svg('arrow-up', 16, 2.25),
    clock=svg('history'), close=svg('x'), check=svg('check', 13), alert=svg('circle-alert'),
    shield=svg('shield-alert', 16), grip=svg('grip-vertical', 14), attach=svg('paperclip'),
    term=svg('terminal', 13), file=svg('file-code-2', 13), newfile=svg('file-plus-2', 13),
    folder=svg('folder-open'), repo=svg('folder-git-2'), commit=svg('git-commit-horizontal'),
    compare=svg('git-compare-arrows'), merge=svg('git-merge'), pr=svg('git-pull-request'),
    undo=svg('undo-2'), redo=svg('redo-2'), upload=svg('upload'), pencil=svg('pencil', 13),
    trash=svg('trash-2', 13), search=svg('search'), server=svg('server'), spark=svg('sparkles'),
    stop=svg('square', 12, 1.75, 'currentColor'), inbox=svg('inbox', 28, 1.25),
    cmd=svg('command'), flow=svg('workflow'), stash=svg('archive'), learn=svg('graduation-cap'),
    chat=svg('message-square', 28, 1.25), bot=svg('bot', 13),
)


def logo(size=18):
    return (f'<svg class="app-logo" width="{size}" height="{size}" viewBox="0 0 512 512" aria-hidden="true">'
            '<rect width="512" height="512" rx="112" fill="var(--logo-bg)"/>'
            '<circle cx="256" cy="256" r="152" fill="none" stroke="var(--logo-ring)" stroke-width="36"/>'
            '<path d="M222 188L296 256L222 324" fill="none" stroke="var(--logo-mark)" stroke-width="36" '
            'stroke-linecap="round" stroke-linejoin="round"/></svg>')


# ---------------------------------------------------------------- page shell

FONTS = ''  # the fonts are bundled: see the @font-face rules in styles.css

SCROLL_END = ('<script>document.querySelectorAll("[data-end]").forEach(function(e){e.scrollTop=e.scrollHeight})'
              '</script>')


def page(title, body, theme='dark', detail=None, root_cls=''):
    tb = ('<header class="titlebar">' + logo() + '<span class="titlebar-name">acportal</span>'
          + (f'<span class="titlebar-detail ell">{esc(detail)}</span>' if detail else '') + '</header>')
    return ('<!doctype html>\n<html lang="en" data-theme="' + theme + '">\n<head>\n<meta charset="utf-8">\n'
            '<meta name="viewport" content="width=device-width, initial-scale=1">\n'
            f'<title>{esc(title)}</title>\n{FONTS}\n<link rel="stylesheet" href="styles.css">\n'
            '</head>\n<body>\n<div id="root"' + (f' class="{root_cls}"' if root_cls else '') + '>'
            + tb + body + '</div>\n' + SCROLL_END + '\n</body>\n</html>\n')


# ---------------------------------------------------------------- shared pieces

def kbd(t, cls=''):
    return f'<span class="kbd{(" " + cls) if cls else ""}">{esc(t)}</span>'


def host_switch(ms, badge=0, open_=False, popover=''):
    b = f'<span class="badge" title="Waiting for your approval on all hosts">{badge}</span>' if badge else ''
    return (
        '<div class="host-head"><div class="host-switcher">'
        f'<button class="host-switch{" open" if open_ else ""}" aria-haspopup="menu">'
        '<span class="host-avatar s-connected">B<span class="dot green"></span></span>'
        '<span class="grow host-lines"><span class="host-name ell">BASINAKOS</span>'
        f'<span class="host-state s-connected">Connected<span class="host-facts"> · {ms} ms</span></span></span>'
        f'{b}<span class="chev-ic">{I.down}</span></button>{popover}</div></div>')


def session_row(title, kind, note='', badge=0, hover=False):
    """kind: ended | amber | green. An ended session has a Resume button; the others are live."""
    if kind == 'ended':
        dot, tail = 'red', '<button class="row-resume">Resume</button>'
        cls = ' ended'
    else:
        dot, cls = kind, ''
        tail = (f'<span class="row-note">{esc(note)}</span>' if note else '')
        tail += f'<span class="badge">{badge}</span>' if badge else ''
    active = ' active' if kind != 'ended' else ''
    if hover:
        active += ' is-hover'
    return (f'<div class="row session nested{active}{cls}"><span class="dot {dot}"></span>'
            f'<span class="grow title">{esc(title)}</span>{tail}</div>')


def sidebar(ms=39, rows=None, count=3, ended='collapsed', badge=0, host_pop='', host_open=False,
            host_hover=False):
    """ended: collapsed ("N ended") | open ("Ended" with its sessions listed above) | none."""
    rows = rows or []
    ended_row = ''
    if ended == 'collapsed':
        ended_row = ('<div class="row compact ended-row" role="button"><span class="chev-ic shut">' + I.down
                     + f'</span><span class="grow ell">{count} ended</span><button class="row-resume">Clear</button></div>')
    elif ended == 'open':
        ended_row = ('<div class="row compact ended-row" role="button"><span class="chev-ic">' + I.down
                     + '</span><span class="grow ell">Ended</span><button class="row-resume">Clear</button></div>')
    head = host_switch(ms, badge, host_open, host_pop)
    if host_hover:
        head = head.replace('class="host-switch"', 'class="host-switch open"')
    return (
        '<aside class="side">' + head +
        '<div class="side-actions">'
        f'<button class="btn full new-btn">{I.plus}New session{kbd("Ctrl N")}</button>'
        f'<button class="btn full multi-btn" title="Same prompt to 2-3 agents, each in its own worktree; compare and merge one">{I.flow}Run on several agents</button>'
        f'<button class="side-search">{I.search}<span class="grow">Search or command</span>{kbd("Ctrl K")}</button>'
        '</div>'
        '<div class="list"><div class="host-group live"><div class="project-group">'
        f'<div class="row compact project-row" role="button"><span class="chev-ic">{I.down}</span>'
        f'<span class="row-ic">{I.folder}</span><span class="grow ell">acportal</span><span class="count">{count}</span>'
        f'<button class="icon-btn project-git">{I.branch}</button><button class="icon-btn project-new">{I.plus}</button></div>'
        + ''.join(rows) + ended_row +
        '</div></div></div>'
        f'<div class="side-foot"><button class="foot-btn">{I.gear}Settings</button></div></aside>')


def statusbar(term=False, browser=False):
    return ('<footer class="statusbar" aria-label="Panels"><span class="grow"></span>'
            f'<button class="statusbar-toggle{" on" if term else ""}">{I.term}Terminal {kbd("Ctrl `")}</button>'
            f'<button class="statusbar-toggle{" on" if browser else ""}">{I.globe}Browser {kbd("Ctrl Shift B")}</button></footer>')


def term_dock(height=300, width_note=''):
    def c(cls, t):
        return f'<span class="{cls}">{t}</span>'
    line = (c('xt-green', 'basin@basinakos') + ' ' + c('xt-purple', 'MINGW64') + ' ' + c('xt-yellow', '~/acportal/.local/real-app-demo/calc')
            + ' ' + c('xt-cyan', '(master)'))
    return (
        f'<section class="term-dock" style="height:{height}px" aria-label="Terminal"><div class="term-resize"></div>'
        f'<div class="term-bar"><span class="term-title">{I.term}Terminal <span class="sub">on BASINAKOS</span></span>'
        f'<div class="term-tabs"><span class="term-tab on"><button>bash 1</button><button class="term-x">{I.close}</button></span></div>'
        f'<span class="term-new"><button class="icon-btn">{I.plus}</button><button class="icon-btn term-pick">{I.down}</button></span>'
        f'<button class="icon-btn">{I.close}</button></div>'
        '<div class="term-stage"><div class="term-view"><div class="term-box xterm-box">'
        f'<div class="xt-line">{line}</div><div class="xt-line"><b>$</b></div></div></div></div></section>')


def app(side, stack, right='', cls='', style=''):
    cls = 'app' + (' no-right' if not right and 'browser-side' not in cls else '') + (' ' + cls if cls else '')
    st = f' style="{style}"' if style else ''
    return f'<div class="{cls}"{st}>{side}<div class="main-stack">{stack}</div>{right}</div>'


# ---------------------------------------------------------------- new session page

def chip(label, icon='', on=False, pop='', dot=''):
    d = f'<span class="dot {dot}"></span>' if dot else ''
    return (f'<span class="ctx-wrap"><button class="ctx-chip{" on" if on else ""}">{d}{icon}<span class="ell">{esc(label)}</span>{I.down}</button>{pop}</span>')


def ctx_item(label, sub='', on=False, mono=False):
    lab = f'<span class="mono">{esc(label)}</span>' if mono else esc(label)
    return (f'<button class="ctx-item{" on" if on else ""}"><span class="grow ell">{lab}</span>'
            + (f'<span class="sub">{esc(sub)}</span>' if sub else '')
            + (f'<span class="tick">{I.check}</span>' if on else '') + '</button>')


def ns_main(*, text='', target='', folder_label='No project', branch='...', pop='', pop_on=False,
            earlier=False, focus=False, sendable=False, show_fold=False):
    lead = (f'New session on BASINAKOS in {esc(target)}. Change anything with the chips below.' if target
            else 'New session on BASINAKOS in&nbsp; no-project. Change anything with the chips below.')
    area = (f'<textarea readonly rows="2" placeholder="Ask Goose to build, fix or explain something…">{esc(text)}</textarea>')
    chips = (chip('BASINAKOS', dot='green')
             + chip(folder_label, I.folder, on=pop_on, pop=pop)
             + chip(branch, I.branch)
             + chip('Supervised', I.shield)
             + (chip('Earlier', I.clock) if earlier else '')
             + '<span class="ctx-switch">Worktree<button class="switch" role="switch" aria-checked="false"><span></span></button></span>')
    send_cls = 'send' + ('' if sendable else ' is-disabled')
    return (
        '<div class="main ns-main"><div class="ns-center">'
        '<h1 class="ns-title">What should we work on?</h1>'
        f'<div class="sub ns-lead">{lead}</div>'
        f'<div class="ns-composer{" is-focus" if focus else ""}">{area}'
        f'<div class="ns-bar"><button class="icon-btn round" aria-label="Attach files">{I.plus}</button>'
        f'<span class="ctx-wrap"><button class="ctx-chip">{I.bot}<span class="ell">Goose</span>{I.down}</button></span>'
        f'<span class="ctx-wrap"><button class="ctx-chip"><span class="ell">ds/deepseek-flash · off</span>{I.down}</button></span>'
        '<span class="grow"></span><span class="hint-keys">Enter to send · Shift Enter new line</span>'
        f'<button class="{send_cls}" aria-label="Start session and send">{I.send}</button></div></div>'
        f'<div class="ctx-row" role="toolbar">{chips}</div>'
        '</div><div class="ns-keys sub">Ctrl N new session · Ctrl K commands · Shift Enter new line</div></div>')


# ---------------------------------------------------------------- conversation pieces

HL_KW = '<span class="hljs-keyword">{}</span>'
HL_T = '<span class="hljs-title function_">{}</span>'


def code_py(lines):
    return '\n'.join(lines)


def diff_view(path='calc.py', rows=None, shut=False):
    """The app's DiffView: header that folds the card, line-number gutters, signs and a skip row."""
    rows = rows or [
        ('ctx', 1, 1, 'def add(a, b):'),
        ('del', 2, '', '    return a - b'),
        ('add', '', 2, '    return a + b'),
        ('ctx', 3, 3, ''),
        ('ctx', 4, 4, ''),
        ('ctx', 5, 5, 'def multiply(a, b):'),
        ('skip',),
    ]
    trs = ''
    for r in rows:
        if r[0] == 'skip':
            trs += '<tr class="skip"><td colspan="4">⋯</td></tr>'
        else:
            kind, a, b, t = r
            sign = '+' if kind == 'add' else '−' if kind == 'del' else ''
            trs += (f'<tr class="{kind if kind != "ctx" else ""}"><td class="n">{a}</td><td class="n">{b}</td>'
                    f'<td class="sign">{sign}</td><td class="t">{esc(t)}</td></tr>')
    return (
        f'<div class="diff{" collapsed" if shut else ""}"><div class="diff-head foldable" role="button">'
        f'<span class="diff-path"><span class="chev-ic">{I.down}</span>{I.file}<span class="mono">{esc(path)}</span></span>'
        '<span class="diff-stat mono"><span class="green">+1</span><span class="red">−1</span>'
        '<span class="bar"><i class="g"></i><i class="g"></i><i class="g"></i><i></i><i></i></span></span></div>'
        f'<div class="diff-body mono"><table><tbody>{trs}</tbody></table></div></div>')


def tool_row(title, status='completed', icon='file', diff=''):
    word = {'completed': 'Done', 'pending': 'Pending', 'in_progress': 'Running'}[status]
    ic = I.term if icon == 'term' else I.file
    return (f'<div><div class="tool"><span class="tool-ic">{ic}</span><span class="tool-title">{esc(title)}</span>'
            f'<span class="status {status}"><span class="st-dot"></span>{word}</span></div>'
            + (f'<div style="margin-top:10px">{diff}</div>' if diff else '') + '</div>')


def tool_with_diff(title, status, icon='file'):
    return tool_row(title, status, icon, diff_view())


def user_bubble(text):
    return f'<div class="bubble-user">{esc(text)}</div>'


def answered(title, src='host · Allow write once'):
    return (f'<div class="answered"><span class="ans-ic green">{I.check}</span><span class="tool-title">{esc(title)}</span>'
            f'<span class="sub">{esc(src)}</span></div>')


def thought(text):
    return f'<div class="thought">{esc(text)}</div>'


def steps_head(text):
    return (f'<div class="steps"><button class="steps-head" aria-expanded="false"><span class="chev-ic shut">{I.down}</span>'
            f'<span>{esc(text)}</span></button></div>')


def working(secs):
    return f'<div class="working"><span class="pulse"></span>Goose is working…<span class="sub"> {esc(secs)}</span></div>'


def approval_write():
    return (
        f'<div class="card approval"><div class="card-h"><span class="approval-ic">{I.shield}</span>'
        '<div class="grow"><h3>Write calc.py</h3><div class="sub">Host consent · host-filesystem</div></div>'
        '<span class="tag grey">edit</span></div>'
        f'<div style="margin-top:10px">{diff_view()}</div>'
        '<div class="approval-foot"><span class="sub grow">The agent waits for your answer.</span>'
        '<button class="btn ghost">Deny</button><button class="btn primary is-focus">Allow write once</button></div></div>')


def approval_shell():
    return (
        f'<div class="card approval danger"><div class="card-h"><span class="approval-ic">{I.term}</span>'
        '<div class="grow"><h3>Run a shell command</h3><div class="sub">Host consent · host-shell-command · in '
        '<span class="mono">\\\\?\\C:\\Users\\basin\\acportal\\.local\\real-app-demo\\calc</span></div></div>'
        '<span class="tag red">Shell</span></div>'
        '<div class="sub2" style="margin-top:8px">Run shell command</div>'
        '<pre class="cmd-block mono"><div><span class="prompt">$</span>python -m unittest -v test_calc.py</div></pre>'
        '<div class="approval-foot"><span class="sub grow">Runs these exact lines once on the host in cmd.exe.</span>'
        '<button class="btn ghost">Deny</button><button class="btn primary is-focus">Run this shell line once</button></div></div>')


MD_PY = ('<div class="md-code-wrap"><div class="md-code-bar"><span>python</span><button class="md-copy">Copy</button></div>'
         '<pre class="md-code mono"><code>'
         '<span class="hljs-keyword">def</span> <span class="hljs-title function_">add</span>(a, b):\n'
         '    <span class="hljs-keyword">return</span> a + b</code></pre></div>')
MD_OUT = ('<div class="md-code-wrap"><div class="md-code-bar"><span>text</span><button class="md-copy">Copy</button></div>'
          '<pre class="md-code mono"><code>test_add (test_calc.CalcTests.test_add) ... ok\n'
          'test_multiply (test_calc.CalcTests.test_multiply) ... ok\nRan 2 tests in 0.000s\nOK</code></pre></div>')


def agent_msg():
    return (
        f'<div class="agent-msg"><div class="agent-name">{I.bot}Goose</div><div class="agent-text"><div class="md">'
        '<p>Both tests pass now.</p>'
        '<p><strong>Bug:</strong> in <code class="ic">calc.py</code>, <code class="ic">add</code> was implemented as '
        '<code class="ic">return a - b</code> instead of <code class="ic">return a + b</code> — so '
        '<code class="ic">add(2, 3)</code> returned <code class="ic">-1</code> instead of <code class="ic">5</code>.</p>'
        f'<p><strong>Fix:</strong></p>{MD_PY}<p><strong>Verified:</strong></p>{MD_OUT}</div></div></div>')


def topbar(title='Fix calc.py test bug', state='Running', live=True, narrow=False, term_on=False, path='…/real-app-demo/calc'):
    return (
        '<div class="topbar"' + (' style="overflow:hidden"' if narrow else '') + '><div class="topbar-title"><h1>' + esc(title) + '</h1>'
        f'<div class="sub crumbs"><span>Goose</span><span class="{"live-word" if live else ""}">{state}</span>'
        f'<span class="mono ell">{esc(path)}</span></div></div>'
        f'<button class="branch-pill mono">{I.branch}master</button>'
        f'<span class="ctx-wrap"><button class="select-btn"><span class="ell">ds/deepseek-flash</span>{I.down}</button></span>'
        f'<span class="vsep"></span><button class="icon-btn{" on" if term_on else ""}">{I.term}</button>'
        f'<button class="icon-btn">{I.globe}</button><button class="icon-btn">{I.compare}</button>'
        f'<button class="icon-btn danger">{I.close}</button></div>')


def composer(busy=False):
    act = (f'<button class="btn stop">{I.stop}Stop</button>' if busy
           else f'<button class="send is-disabled" aria-label="Send">{I.send}</button>')
    return (
        '<div class="composer" style="position:relative"><textarea readonly rows="1" placeholder="Message Goose…"></textarea>'
        f'<div class="bar"><button class="icon-btn">{I.attach}</button><button class="icon-btn">{I.clock}</button>'
        f'<span style="flex:1"></span><span class="hint-keys">{kbd("Enter")} send · {kbd("Shift Enter")} new line</span>{act}</div></div>')


def conversation(items, end=True):
    return f'<div class="scroll"{" data-end" if end else ""}><div class="convo">{"".join(items)}</div></div>'


def right_panel(files=1, usage='US$0.0009', files_added=None):
    n = files
    rows = ''.join(
        f'<div class="file-row"><span class="st mono m">M</span><span class="mono grow ell">calc.py</span>'
        '<span class="mono sub"><span class="green">+1</span> <span class="red">−1</span></span></div>' for _ in range(n))
    return (
        '<aside class="right"><section class="panel"><div class="panel-h">' + I.file + '<span>Changed files</span>'
        f'<span class="diff-stat mono"><span class="green">+{n}</span><span class="red">−{n}</span></span></div>'
        + rows + f'<button class="btn full" style="margin-top:10px">{I.compare}Review all changes</button></section>'
        f'<section class="panel"><div class="panel-h">{I.server}<span>Usage</span></div>'
        '<div class="meter"><i style="width:1.3%"></i></div>'
        f'<div class="hint">13k of 1000k context · {usage}</div></section></aside>')


def conversation_page(items, *, ms=16, state='Running', live=True, files=1, usage='US$0.0009', composer_busy=True,
                      rows=None, badge=0, dock=False, term_on=False, ended='open', count=4, right=True, end=True):
    side = sidebar(ms, rows, count, ended, badge)
    stack = (f'<div class="main">{topbar(state=state, live=live, term_on=term_on)}{conversation(items, end)}{composer(composer_busy)}</div>'
             + (term_dock() if dock else '') + statusbar(term=dock))
    return app(side, stack, right_panel(files, usage) if right else '')


ENDED = [session_row('Goose in acportal', 'ended'), session_row('What cool stuff …', 'ended'), session_row('Hello?', 'ended')]
def rows_running(badge=1):
    return ENDED + [session_row('Fix calc.py test bug', 'amber', badge=badge)]
ROWS_DONE = ENDED + [session_row('Fix calc.py test bug', 'green', note='1m')]


# ---------------------------------------------------------------- git

def git_header(sub_right=''):
    return (
        f'<div class="topbar"><button class="icon-btn" aria-label="Back">{I.back}</button>'
        '<div class="topbar-title"><h1>Git <span class="h-dim">·</span> calc</h1>'
        '<div class="sub">not published · <span class="mono">\\\\?\\C:\\Users\\basin\\acportal\\.local\\real-app-demo\\calc</span> · run by the host</div></div>'
        f'<div class="pop-anchor"><button class="select-btn">{I.branch}<span class="mono">master</span>{I.down}</button></div>'
        '<select class="select-btn mono" title="Repository or worktree"><option>calc · session</option></select>'
        f'<button class="toggle-chip">{I.learn} Learn git</button><button class="icon-btn">{I.refresh}</button></div>')


def git_tabs(on):
    tabs = [('Changes', 1), ('Branches', 1), ('History', 0), ('Worktrees', 1), ('Stash', 0), ('Learn', 0)]
    b = ''.join(f'<button class="{"on" if t == on else ""}">{t}' + (f'<span class="count">{n}</span>' if n else '') + '</button>'
                for t, n in tabs)
    return (f'<div class="tabs">{b}<span style="flex:1"></span>'
            f'<span class="gitbtn"><button class="btn ghost is-disabled" disabled>{I.undo}Undo</button></span>'
            f'<span class="gitbtn"><button class="btn ghost is-disabled" disabled>{I.redo}Redo</button></span></div>')


def git_page(tab, body, dock=True):
    side = sidebar(16, ROWS_DONE, 4, 'open')
    stack = (f'<div class="main">{git_header()}{git_tabs(tab)}<div class="scroll">{body}</div></div>'
             + (term_dock() if dock else '') + statusbar(term=dock))
    return app(side, stack)


def git_changes():
    diff = (
        '<div class="diff"><div class="diff-head"><span class="head-left"><input type="checkbox" class="pick">'
        f'<button class="diff-path fold-btn"><span class="chev-ic">{I.down}</span>{I.file}<span class="mono">calc.py</span></button></span>'
        '<span class="gitbtn"><button class="btn">Stage file</button></span></div>'
        '<div><div class="hunk-head mono"><span>@@ -1,5 +1,5 @@</span><span class="gitbtn"><button class="btn">Stage hunk</button></span></div>'
        '<div class="diff-body mono"><table><tbody>'
        '<tr><td class="sign"></td><td class="t">def add(a, b):</td></tr>'
        '<tr class="del"><td class="sign">-</td><td class="t">    return a - b</td></tr>'
        '<tr class="add"><td class="sign">+</td><td class="t">    return a + b</td></tr>'
        '<tr><td class="sign"></td><td class="t">def multiply(a, b):</td></tr></tbody></table></div></div></div>')
    left = (
        '<div><div class="fold-bar"><button class="btn ghost sm">Expand all</button><button class="btn ghost sm">Collapse all</button></div>'
        '<div class="group-h"><span>Unstaged</span><span class="count">1</span></div>'
        '<div class="multi-bar"><label class="pick-all"><input type="checkbox" class="pick">Select all</label>'
        '<span class="gitbtn"><button class="btn primary is-disabled" disabled>Stage selected</button></span>'
        '<span class="gitbtn"><button class="btn">Stage all</button></span></div>'
        + diff +
        '<div class="group-h" style="margin-top:18px"><span>Staged</span><span class="count">0</span></div>'
        '<div class="hint">Nothing staged yet. Stage a file or a single hunk to build the next commit.</div></div>')
    commit = (
        f'<div class="card commit-box"><div class="card-h">{I.commit}<h3>Commit</h3></div>'
        '<div class="sub2">Stage changes first</div>'
        '<textarea class="input area" readonly placeholder="Summary of the change (first line ≤ 72 chars)"></textarea>'
        '<div class="counter">0/72</div>'
        f'<div class="btn-row"><button class="btn is-disabled" disabled>{I.spark}Suggest</button>'
        '<select class="select-btn"><option>Goose</option></select><span style="flex:1"></span>'
        '<span class="gitbtn"><button class="btn primary is-disabled" disabled>Commit to master</button></span></div>'
        f'<div class="hint" style="margin-top:12px">Undo of a commit uses <span class="mono">git reset --soft</span>, which keeps your changes staged; '
        f'Redo moves the branch back to it. {kbd("Ctrl Z")} {kbd("Ctrl Shift Z")}</div></div>')
    return f'<div class="git-grid">{left}{commit}</div>'


def git_branches():
    return (
        '<div class="narrow"><div class="card" style="margin-bottom:16px">'
        f'<div class="card-h">{I.branch}<h3>On <span class="mono">master</span></h3><span style="flex:1"></span><span class="tag grey">only on this computer</span></div>'
        f'<div class="btn-row"><span class="sub">Compare with</span><select class="select-btn mono is-disabled"><option></option></select>'
        f'<span class="gitbtn"><button class="btn is-disabled" disabled>{I.compare}Compare</button></span><span style="flex:1"></span>'
        f'<span class="gitbtn"><button class="btn primary">{I.upload}Publish branch</button></span>'
        f'<span class="gitbtn"><button class="btn is-disabled" disabled>{I.pr}Open pull request</button></span></div>'
        '<div class="hint" style="margin-top:8px">Pull request links appear when origin is on GitHub. Push uses the host\'s own git credentials; the app stores none.</div></div>'
        '<div class="group-h"><span>Local branches</span><span class="count">1</span></div>'
        '<div class="list-card"><div class="list-row"><span class="dot green"></span><div class="grow"><div class="mono">'
        'master<span class="tag green" style="margin-left:8px">current</span></div>'
        '<div class="sub">8710a0a · 5 minutes ago</div></div>'
        f'<button class="icon-btn">{I.pencil}</button></div></div>'
        '<div class="btn-row" style="margin-top:14px"><input class="input mono" style="flex:1" readonly placeholder="new-branch-name">'
        '<span class="sub">from</span><select class="select-btn mono"><option>master</option></select>'
        f'<span class="gitbtn"><button class="btn is-disabled" disabled>{I.plus}Create and switch</button></span></div></div>')


def git_history():
    return (
        '<div class="mono graph-list"><div class="graph-row dag-row"><svg class="dag" width="22" height="28" viewBox="0 0 22 28" aria-hidden="true">'
        '<circle cx="11" cy="14" r="6" fill="var(--bg)" stroke="var(--green)" stroke-width="2"/><circle cx="11" cy="14" r="3" fill="var(--green)"/></svg>'
        '<span class="sub">8710a0a</span><span class="ref head">master</span><span class="subj">calc with a failing add test</span>'
        '<span class="sub">demo · 5 minutes ago</span></div></div>')


def git_worktrees():
    return (
        '<div class="narrow"><div class="hint" style="margin-bottom:12px">A worktree is another folder checked out from the same repository, '
        'on its own branch. Agents use them so parallel tasks never edit the same files.</div>'
        '<div class="list-card"><div class="list-row"><span class="dot green"></span><div class="grow">'
        f'<div><span class="branch-pill">{I.branch}master</span><span class="sub"> · main checkout</span></div>'
        '<div class="sub mono">C:/Users/basin/acportal/.local/real-app-demo/calc</div></div><button class="btn">Session here</button></div></div>'
        '<div class="btn-row" style="margin-top:14px"><input class="input mono" style="flex:1" readonly placeholder="new-branch-name">'
        f'<button class="btn is-disabled" disabled>{I.plus}New worktree</button></div>'
        '<div class="hint" style="margin-top:18px">acpd names delegated task branches <span class="mono">acpd/task-…</span>; '
        'merge them from the task view so the host can check the parent folder first.</div></div>')


def git_stash():
    return (
        '<div class="narrow"><div class="hint" style="margin-bottom:12px">The stash is a shelf for unfinished work: it cleans the folder '
        'without committing, and you can bring the changes back later.</div>'
        f'<div class="btn-row" style="margin-top:0"><button class="btn">{I.stash}Stash all changes</button></div>'
        '<div class="list-card" style="margin-top:14px"><div class="list-row sub">The stash is empty.</div></div></div>')


# ---------------------------------------------------------------- settings

SETTINGS = [('General', I.gear), ('Agents', I.bot), ('MCP servers', I.server), ('Hosts', I.term), ('Workspaces', I.folder),
            ('Approvals & review', I.shield), ('Terminal', I.term), ('Browser', I.globe), ('Appearance', I.spark),
            ('Keyboard', I.cmd)]


def settings_page(section, body):
    nav = ''.join(f'<div class="row{" active" if s == section else ""}"><span class="nav-ic">{ic}</span><span>{esc(s)}</span></div>'
                  for s, ic in SETTINGS)
    return (f'<div class="settings"><nav class="settings-nav"><button class="settings-back" aria-label="Back">{I.back}<span>Back to app</span></button>'
            f'<div class="settings-nav-title">Settings</div>{nav}</nav><div class="settings-body">{body}</div></div>')


def set_row(label, hint, *, mono_hint=False, toggle=None, extra=''):
    t = '' if toggle is None else f'<span class="toggle{" on" if toggle else ""}" role="switch"></span>'
    return (f'<div class="set-row"><div style="flex:1"><div>{label}</div><div class="sub{" mono" if mono_hint else ""}">{hint}</div></div>{extra}{t}</div>')


def s_general():
    return ('<h2>General</h2><div class="sub" style="margin-bottom:18px">acportal desktop 0.1.0</div>'
            + set_row('Credential storage', 'System keychain (win32)')
            + set_row('Connection', 'HTTPS with system trust for remote hosts; localhost may use HTTP. Certificate checks cannot be turned off.')
            + set_row('Prompts after a disconnect', 'Never resent automatically; a lost connection does not cancel a turn or approve anything.'))


def s_agents():
    def agent(name, meta, tail=''):
        return ('<div><div class="set-row">' f'<span class="sub">{I.gear}</span><div style="flex:1"><div>{name}</div>'
                f'<div class="sub">{meta}</div></div><button class="pill">Edit</button><button class="pill">Remove</button>'
                '<button class="toggle on" role="switch"></button></div></div>')
    return (f'<div style="display:flex;justify-content:space-between;align-items:center"><h2>Agents</h2><button class="pill">{I.plus} Add agent</button></div>'
            '<div class="sub" style="margin-bottom:14px">Agents registered on BASINAKOS. Any agent that speaks ACP over stdio.</div>'
            + agent('grok', '<span class="mono">Grok</span> · own tools · sandbox off · available')
            + agent('Goose', '<span class="mono">goose</span> · sandbox off · running'))


def s_mcp():
    return (f'<div style="display:flex;justify-content:space-between;align-items:center"><h2>MCP servers</h2><button class="pill">{I.plus} Add server</button></div>'
            '<div class="sub" style="margin-bottom:14px">Saved on this computer and sent with each new session. The agent runs them on the host; HTTP and SSE need agent support.</div>'
            '<div class="sub2">No servers yet.</div>')


def s_hosts():
    return (
        f'<div style="display:flex;align-items:center;gap:8px"><h2 style="flex:1">Hosts</h2><button class="btn primary">{I.plus}Add host</button></div>'
        '<div class="sub" style="margin-bottom:6px">Each host is paired with its own one-use code and keeps its own token. The app connects to all of them at once; '
        'drag rows to set the order used in the sidebar and switcher.</div>'
        '<div class="agg-line s-connected"><span class="dot green"></span>Connected</div>'
        '<div class="set-row host-row"><span class="dot green"></span><div style="flex:1;min-width:0">'
        '<div class="host-row-name">BASINAKOS<span class="cur-tag">current</span></div><div class="sub mono ell">127.0.0.1:8765</div>'
        '<div class="host-item-state s-connected">Connected<span> · 14 ms</span><span> · acpd 0.1.0</span><span> · 2 agents</span><span> · token in keychain</span></div>'
        '<div class="feat-tags"><span class="feat-tag">git</span><span class="feat-tag">worktrees</span><span class="feat-tag">forms</span></div></div>'
        f'<div class="host-row-acts"><button class="icon-btn" title="Reconnect">{I.refresh}</button><button class="icon-btn" title="Rename">{I.pencil}</button>'
        f'<button class="icon-btn" disabled>{I.up}</button><button class="icon-btn" disabled>{I.down}</button><button class="icon-btn danger">{I.trash}</button></div></div>')


def s_workspaces():
    return ('<h2>Workspaces</h2><div class="sub" style="margin-bottom:14px">Folders agents may work in. Folders added here must be inside the host\'s workspace roots.</div>'
            '<div class="set-row"><div style="flex:1"><div>acportal</div><div class="sub mono">C:\\Users\\basin\\acportal</div></div></div>'
            '<div style="display:flex;gap:8px;margin-top:14px"><input class="input mono" style="flex:1" readonly placeholder="/absolute/path/on/host">'
            '<button class="pill is-disabled" disabled>Add folder</button></div>')


def s_approvals():
    return ('<h2>Approvals &amp; review</h2><div class="sub" style="margin-bottom:14px">How decisions reach you. Rules and smart review are configured on the host.</div>'
            + set_row('Shell commands', 'Shown line by line with the host\'s auto-review verdict. "Allow once" runs the exact lines once.')
            + set_row('Delegated tasks', 'Requests a supervising agent may not decide come here with a badge on the task tree.')
            + set_row('Host smart review', '[shell_review.&lt;agent&gt;] in the host config', mono_hint=True))


def s_terminal():
    ports = ''.join(f'<span class="chip mono">{p}<button aria-label="Remove port {p}">{I.close}</button></span>' for p in (3000, 4173, 5173, 5174, 8000, 8080))
    return ('<h2>Terminal</h2><div class="sub">Real shells on BASINAKOS, opened from the desktop app or a phone. They are off until you turn them on.</div>'
            + set_row('Allow terminals on this host', 'On: a paired device can open a shell in an allowed folder.', toggle=True)
            + '<div class="sub" style="margin-top:6px">Set from an app. <button class="linkish">Use the host\'s config.toml instead</button></div>'
            '<h3 class="set-h">Preview on the phone</h3>'
            '<div class="sub" style="margin-bottom:8px">Lets a paired phone show a dev server running on this computer, through the host. Only the ports below, '
            'only pages (no form posts), and whatever those servers show is visible to every paired phone.</div>'
            + set_row('Allow preview', 'Off.', toggle=False)
            + f'<div class="chips">{ports}</div>'
            f'<form class="set-add"><input class="input mono" readonly placeholder="5173"><button class="btn is-disabled" type="button" disabled>{I.plus}Add</button></form>')


def s_browser():
    chips = ''.join(f'<span class="chip mono">{a}<button aria-label="Remove {a}">{I.close}</button></span>' for a in ('localhost', '127.0.0.1'))
    return ('<h2>Browser</h2><div class="sub">A page on this computer or a dev server on the host opens in the app. The page runs apart from the app and cannot reach it.</div>'
            '<h3 class="set-h">Allowed sites</h3>'
            '<div class="sub" style="margin-bottom:8px">localhost and 127.0.0.1 are always allowed. Links to other sites open in your own browser.</div>'
            f'<div class="chips">{chips}</div>'
            f'<form class="set-add"><input class="input mono" readonly placeholder="docs.example.com"><button class="btn is-disabled" type="button" disabled>{I.plus}Add</button></form>'
            + set_row('Open other links in my browser', 'Links to sites that are not on the list. Off: they are refused.', toggle=True)
            + set_row('Remember sign-ins', 'Keeps cookies between launches. Off: every launch starts clean. Applies the next time the browser opens.', toggle=False)
            + set_row('Downloads', 'Let pages download files. Off: downloads are blocked.', toggle=False))


def s_appearance(on='dark'):
    prev = {'dark': 'background:#0a0a0a', 'light': 'background:#fafafa',
            'system': 'background:linear-gradient(135deg,#0a0a0a 50%,#fafafa 50%)'}
    bar1 = {'dark': '#eee', 'light': '#111', 'system': '#eee'}
    cards = ''.join(
        f'<div class="theme-card{" on" if t == on else ""}"><div class="pv" style="{prev[t]};border:1px solid var(--line)">'
        f'<div style="margin:12px;width:60px;height:10px;border-radius:5px;background:{bar1[t]}"></div>'
        '<div style="margin:0 12px;width:90px;height:8px;border-radius:4px;background:#72d5b0;opacity:.8"></div></div>'
        f'<div style="margin-top:8px;text-transform:capitalize">{t}</div></div>' for t in ('dark', 'light', 'system'))
    return ('<h2>Appearance</h2><div class="sub">Matches the Android app: near-black, near-white, no accent colour.</div>'
            f'<div class="theme-cards">{cards}</div>')


def s_keyboard():
    items = [('Command palette', 'Ctrl/⌘ K'), ('New session', 'Ctrl/⌘ N'), ('Settings', 'Ctrl/⌘ ,'), ('Review changes', 'Ctrl/⌘ D'),
             ('Show or hide the browser', 'Ctrl/⌘ Shift B'), ('Stop the current turn', 'Esc'), ('Send', 'Enter'), ('New line', 'Shift Enter')]
    return '<h2>Keyboard</h2>' + ''.join(f'<div class="set-row"><span style="flex:1">{a}</span>{kbd(k)}</div>' for a, k in items)


# ---------------------------------------------------------------- overlays

def palette(results=16):
    def it(ic, label, hint='', sel=False):
        return (f'<div class="item{" sel" if sel else ""}"><span class="pal-ic">{ic}</span><span class="grow">{esc(label)}</span>'
                + (kbd(hint) if hint else '') + '</div>')
    grp = lambda t: f'<div class="group">{t}</div>'
    items = (grp('Actions') + it(I.plus, 'New session...', 'Ctrl N', True) + it(I.spark, 'Run on several agents...')
             + it(I.branch, 'Open Git (changes, history, worktrees, stash)', 'Ctrl G') + it(I.spark, 'Show or hide the terminal', 'Ctrl `')
             + it(I.spark, 'Open the browser (preview a page)', 'Ctrl Shift B') + it(I.compare, 'Review changes in this session', 'Ctrl D')
             + it(I.stop, 'Stop the current turn', 'Esc') + it(I.gear, 'Open settings', 'Ctrl ,')
             + grp('Hosts') + it(I.server, 'Add host...') + it(I.server, 'Reconnect all hosts') + it(I.server, 'Manage hosts')
             + grp('Appearance') + it(I.spark, 'Theme: dark') + it(I.spark, 'Theme: light') + it(I.spark, 'Theme: follow the system')
             + grp('Sessions') + it(I.chat.replace('width="28" height="28"', 'width="15" height="15"'), 'Fix calc.py test bug')
             + it(I.chat.replace('width="28" height="28"', 'width="15" height="15"'), 'Hello?'))
    return ('<div class="overlay"><div class="palette">'
            f'<div class="palette-search">{I.search}<input placeholder="Type a command or search sessions..." readonly>{kbd("Esc")}</div>'
            f'<div class="palette-list" style="max-height:420px;overflow-y:auto;padding-bottom:8px">{items}</div>'
            f'<div class="palette-foot"><span><span class="kbd">↑</span><span class="kbd">↓</span> move</span><span>{kbd("Enter")} run</span>'
            f'<span class="grow"></span><span>{results} results</span></div></div></div>')


def multi_modal():
    return (
        '<div class="overlay"><div class="modal" style="width:640px">'
        f'<div class="modal-h"><h3>Run on several agents</h3><button class="icon-btn" aria-label="Close">{I.close}</button></div>'
        '<div class="sub2">Every agent gets the same prompt on its own new branch in its own worktree, so nothing touches your checkout until you merge the one you pick.</div>'
        '<div class="field"><label>Prompt</label><textarea class="input is-focus" readonly rows="4" placeholder="Add a --verbose flag to the CLI and a test for it"></textarea></div>'
        '<div class="grid2"><div class="field"><label>Folder</label><select class="input mono"><option>C:\\Users\\basin\\acportal</option></select></div>'
        '<div class="field"><label>Sub-folder <span class="sub">optional</span></label><input class="input mono" readonly placeholder="e.g. notes"></div></div>'
        '<div class="field"><label>Start from branch</label><select class="input mono"><option>main</option></select></div>'
        '<div class="field"><label>Agents</label><div class="grid3"><select class="input"><option>grok</option></select>'
        '<select class="input"><option>Goose</option></select><select class="input"><option>— none —</option></select></div>'
        '<div class="hint">Runs get branches like <span class="mono">run/task-xxxx-1-grok</span>. Approvals from every run show up in the sidebar and as notifications.</div></div>'
        '<div style="display:flex;gap:8px;margin-top:14px;align-items:center"><span class="sub grow"></span><button class="btn ghost">Cancel</button>'
        f'<button class="btn primary">{I.flow}Start 2 runs</button></div></div></div>')


def host_popover():
    return (
        '<div class="popover host-pop" role="menu"><div class="host-pop-head"><span>Hosts</span>'
        '<span class="agg s-connected"><span class="dot green"></span>Connected</span></div>'
        '<div class="pop-list"><div class="host-item on"><button class="host-item-main"><span class="dot green"></span><span class="grow">'
        '<span class="host-item-name">BASINAKOS<span class="cur-tag">current</span></span><span class="sub mono ell">127.0.0.1:8765</span>'
        '<span class="host-item-state s-connected">Connected<span> · 32 ms</span><span> · acpd 0.1.0</span><span> · 2 agents</span></span>'
        '<span class="feat-tags"><span class="feat-tag">git</span><span class="feat-tag">worktrees</span><span class="feat-tag">forms</span></span></span></button>'
        f'<div class="host-item-acts"><button class="icon-btn" title="Reconnect BASINAKOS">{I.refresh}</button></div></div></div>'
        f'<div class="host-pop-foot"><button class="btn sm">{I.plus}Add host</button><span class="grow"></span>'
        f'<button class="btn sm ghost">{I.gear}Manage hosts</button></div></div>')


def browser_panel():
    return (
        '<section class="browser side" aria-label="Browser"><div class="browser-resize"></div>'
        f'<div class="browser-tabs"><span class="browser-tab on"><button>New tab</button><button class="browser-tab-x">{I.close}</button></span>'
        f'<button class="icon-btn">{I.plus}</button></div>'
        f'<div class="browser-bar"><button class="icon-btn" disabled>{I.back}</button><button class="icon-btn" disabled>{I.forward}</button>'
        f'<button class="icon-btn" disabled>{I.refresh}</button>'
        f'<form class="browser-address"><span class="browser-scope">{I.home}</span><input class="mono" placeholder="Address (Ctrl L)" readonly></form>'
        '<button class="browser-zoom" disabled>100%</button>'
        f'<button class="icon-btn" disabled>{I.search}</button><button class="icon-btn" disabled>{I.code}</button>'
        f'<button class="icon-btn" disabled>{I.external}</button>'
        f'<div class="browser-send"><button class="btn primary" disabled>{I.spark}Send to agent {I.down}</button></div>'
        f'<button class="icon-btn">{I.dockBottom}</button><button class="icon-btn">{I.close}</button></div>'
        '<div class="browser-area"><div class="browser-empty"><div class="empty-ic">' + I.globe + '</div>'
        '<h2>Preview a page</h2><p>Type an address above. Pages on this computer open here; other sites open in your own browser.</p>'
        '<div class="browser-quick"><button class="btn mono">localhost:5173</button><button class="btn mono">localhost:3000</button>'
        '<button class="btn mono">localhost:8080</button></div></div></div>'
        '<div class="browser-foot">Isolated: no sign-ins kept</div></section>')


# ---------------------------------------------------------------- the screens

NO_PROJECT_POP = ('<div class="ctx-pop wide down"><div class="ctx-h">Folder</div>' + ctx_item('C:\\Users\\basin\\acportal', mono=True)
                  + ctx_item('No project', 'new empty folder', on=True)
                  + '<div class="hint ctx-pad">The host makes a new empty folder for this session, outside every project. '
                    'Use it for questions and one-off tasks.</div></div>')


def folder_pop(sub=''):
    box = (f'<input class="input mono ctx-input is-focus" readonly value="{esc(sub)}">' if sub
           else '<input class="input mono ctx-input" readonly placeholder="e.g. calc">')
    return ('<div class="ctx-pop wide down"><div class="ctx-h">Folder</div>' + ctx_item('C:\\Users\\basin\\acportal', on=True, mono=True)
            + ctx_item('No project', 'new empty folder') + '<div class="ctx-sep"></div>'
            '<label class="ctx-h">Sub-folder (optional)</label>' + box + '</div>')


WS = 'C:\\Users\\basin\\acportal'
SUB = WS + '/.local/real-app-demo/calc'
TITLE = 'Fix calc.py test bug'


def s01(): return app(sidebar(39, [], 3, 'collapsed'), ns_main() + statusbar())
def s02(): return s01() + palette()


def s03():
    side = sidebar(14, ENDED, 3, 'open').replace('row session nested ended"', 'row session nested ended is-hover"', 1)
    return app(side, ns_main() + statusbar())


def ns_page(ms, **kw):
    return app(sidebar(ms, ENDED, 3, 'open'), ns_main(**kw) + statusbar())


def s04(): return ns_page(21, pop_on=True, pop=NO_PROJECT_POP)
def s05(): return ns_page(15, target=WS, folder_label=WS, branch='main', earlier=True, pop_on=True, pop=folder_pop())
def s06(): return ns_page(43, target=SUB, folder_label=SUB, branch='master', earlier=True, pop_on=True, pop=folder_pop('.local/real-app-demo/calc'))
def s07():
    return ns_page(15, text='The test in test_calc.py fails. Find the bug in calc.py, fix it, then run the tests to check.',
                   target=SUB, folder_label=SUB, branch='master', earlier=True, focus=True, sendable=True)


def s08():
    items = [user_bubble('The test in test_calc.py fails. Find the bug in calc.py, fix it, then run the tests to check.'),
             tool_with_diff('Write calc.py', 'pending'), approval_write(), working('35s')]
    return conversation_page(items, ms=18, rows=rows_running(), badge=1, files=1, usage='US$0.0009')


def shell_step(ms, secs):
    items = [user_bubble('The test in test_calc.py fails. Find the bug in calc.py, fix it, then run the tests to check.'),
             tool_with_diff('Write calc.py', 'pending'), answered('Write calc.py'),
             thought('Now run the tests. Shell failed earlier. Try again.'),
             tool_row('shell · python -m unittest -v test_calc.py', 'pending', 'term'),
             tool_row('Run shell command', 'pending', 'term'), approval_shell(), working(secs)]
    return conversation_page(items, ms=ms, rows=rows_running(), badge=1, files=2, usage='US$0.0010')


def s09(): return shell_step(17, '54s')
def s10(): return shell_step(16, '1m 00s')


def finished_items():
    return [user_bubble('The test in test_calc.py fails. Find the bug in calc.py, fix it, then run the tests to check.'),
            tool_with_diff('Write calc.py', 'completed'), tool_row('Run shell command', 'completed', 'term'),
            steps_head('Worked for 1m 17s · 9 steps'), agent_msg()]


def s11(): return conversation_page(finished_items(), ms=81, state='Ready', live=False, rows=ROWS_DONE, composer_busy=False, files=2, usage='US$0.0012')
def s12(): return conversation_page(finished_items(), ms=44, state='Ready', live=False, rows=ROWS_DONE, composer_busy=False, files=2, usage='US$0.0012', end=False)


def s13():
    side = sidebar(16, ROWS_DONE, 4, 'open')
    files = ''.join(f'<div class="row"><span class="row-ic">{I.file}</span><div class="grow"><div class="mono ell">calc.py</div></div>'
                    '<span class="mono sub"><span class="green">+1</span> <span class="red">−1</span></span></div>' for _ in range(2))
    main = (f'<div class="main"><div class="topbar"><button class="icon-btn" aria-label="Back">{I.back}</button><div class="topbar-title">'
            f'<h1>Review · {TITLE}</h1><div class="sub">Goose · \\\\?\\C:\\Users\\basin\\acportal\\.local\\real-app-demo\\calc</div></div></div>'
            '<div class="review-grid"><div class="review-files"><div class="row active"><span class="grow">All files</span><span class="count">2</span></div>'
            f'{files}</div><div class="scroll"><div class="review-list">{diff_view()}{diff_view()}</div></div></div></div>')
    return app(side, main + statusbar())


def s14(): return git_page('Changes', git_changes(), dock=False)
def s16(): return git_page('Branches', git_branches())
def s17(): return git_page('History', git_history())
def s18(): return git_page('Worktrees', git_worktrees())
def s19(): return git_page('Stash', git_stash())


def s15():
    side = sidebar(35, ROWS_DONE, 4, 'open')
    narrow_items = [user_bubble('The test in test_calc.py fails. Find the bug in calc.py, fix it, then run the tests to check.'),
                    steps_head('Worked for 1m 17s · 9 steps'), agent_msg()]
    main = (f'<div class="main">{topbar(state="Ready", live=False, narrow=True)}{conversation(narrow_items, end=False)}{composer(False)}</div>'
            + statusbar(browser=True))
    return app(side, main, browser_panel(), cls='browser-side', style='--browser-w:692px')


def s20(): return conversation_page(finished_items(), ms=52, state='Ready', live=False, rows=ROWS_DONE, composer_busy=False, files=2, usage='US$0.0012', dock=True, term_on=True)


def s34(host_hover=True):
    side = sidebar(13, ROWS_DONE, 4, 'open', host_hover=host_hover)
    stack = (f'<div class="main">{topbar(state="Ready", live=False, term_on=True)}{conversation(finished_items())}{composer(False)}</div>'
             + term_dock() + statusbar(term=True))
    return app(side, stack, right_panel(2, 'US$0.0012'))


def s37(): return app(sidebar(17, ROWS_DONE, 4, 'open'), ns_main(target=WS, folder_label=WS, branch='main', earlier=True) + term_dock() + statusbar(term=True)) + multi_modal()


def s38():
    side = sidebar(32, ROWS_DONE, 4, 'open', host_pop=host_popover(), host_open=True)
    return app(side, ns_main(target=WS, folder_label=WS, branch='main', earlier=True) + term_dock() + statusbar(term=True))


def settings(section, body, ):
    return settings_page(section, body)


def s21(): return settings('General', s_general())
def s22(): return settings('Agents', s_agents())
def s23(): return settings('MCP servers', s_mcp())
def s24(): return settings('Hosts', s_hosts())
def s25(): return settings('Workspaces', s_workspaces())
def s26(): return settings('Approvals & review', s_approvals())
def s27(): return settings('Terminal', s_terminal())
def s28(): return settings('Browser', s_browser())
def s29(): return settings('Appearance', s_appearance('dark'))
def s30(): return settings('Keyboard', s_keyboard())
def s31(): return settings('Appearance', s_appearance('light'))
def s32(): return s21()
def s33(): return s22()
def s35(): return s34(host_hover=False) + palette(17)


def s36():
    side = sidebar(17, ROWS_DONE, 4, 'open', host_hover=True)
    return app(side, ns_main(target=WS, folder_label=WS, branch='main', earlier=True, focus=True) + term_dock() + statusbar(term=True))


# (file, page title, title-bar detail, theme, builder)
SCREENS = [
    ('01-home-new-session', 'New session composer', None, 'dark', s01),
    ('02-command-palette', 'Command palette', None, 'dark', s02),
    ('03-sidebar-ended-sessions', 'Sidebar with ended sessions', None, 'dark', s03),
    ('04-new-session-project-picker', 'Project picker', None, 'dark', s04),
    ('05-folder-browser', 'Folder and sub-folder', None, 'dark', s05),
    ('06-new-session-subfolder', 'Sub-folder typed', None, 'dark', s06),
    ('07-composer-with-prompt', 'Prompt typed', None, 'dark', s07),
    ('08-approval-card-write', 'Write approval', TITLE, 'dark', s08),
    ('09-run-finished', 'Run after the write', TITLE, 'dark', s09),
    ('10-approval-card-shell', 'Shell approval', TITLE, 'dark', s10),
    ('11-run-complete-conversation', 'Finished conversation', TITLE, 'dark', s11),
    ('12-conversation-top-tool-steps', 'Conversation top', TITLE, 'dark', s12),
    ('13-review-changes', 'Review changes', TITLE, 'dark', s13),
    ('14-git-panel', 'Git changes', TITLE, 'dark', s14),
    ('15-browser-panel', 'Browser panel', TITLE, 'dark', s15),
    ('16-git-branches', 'Git branches', TITLE, 'dark', s16),
    ('17-git-history', 'Git history', TITLE, 'dark', s17),
    ('18-git-worktrees', 'Git worktrees', TITLE, 'dark', s18),
    ('19-git-stash', 'Git stash', TITLE, 'dark', s19),
    ('20-terminal-dock', 'Terminal dock', TITLE, 'dark', s20),
    ('21-settings', 'Settings: General', TITLE, 'dark', s21),
    ('22-settings-agents', 'Settings: Agents', TITLE, 'dark', s22),
    ('23-settings-mcp-servers', 'Settings: MCP servers', TITLE, 'dark', s23),
    ('24-settings-hosts', 'Settings: Hosts', TITLE, 'dark', s24),
    ('25-settings-workspaces', 'Settings: Workspaces', TITLE, 'dark', s25),
    ('26-settings-approvals-review', 'Settings: Approvals and review', TITLE, 'dark', s26),
    ('27-settings-terminal', 'Settings: Terminal', TITLE, 'dark', s27),
    ('28-settings-browser', 'Settings: Browser', TITLE, 'dark', s28),
    ('29-settings-appearance', 'Settings: Appearance', TITLE, 'dark', s29),
    ('30-settings-keyboard', 'Settings: Keyboard', TITLE, 'dark', s30),
    ('31-settings-appearance-light', 'Settings: Appearance (light)', TITLE, 'light', s31),
    ('32-settings-general-light', 'Settings: General (light)', TITLE, 'light', s32),
    ('33-settings-agents-light', 'Settings: Agents (light)', TITLE, 'light', s33),
    ('34-conversation-run-complete-light', 'Finished conversation (light)', TITLE, 'light', s34),
    ('35-command-palette-light', 'Command palette (light)', None, 'light', s35),
    ('36-home-new-session-light', 'New session composer (light)', None, 'light', s36),
    ('37-run-on-several-agents', 'Run on several agents', None, 'dark', s37),
    ('38-host-switcher', 'Host switcher', None, 'dark', s38),
]


INDEX = """<!doctype html>
<html lang="en" data-theme="dark"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<title>acportal desktop screens</title>
<style>
:root{--bg:#0a0a0a;--fg:#eee;--mut:#8d8d94;--line:rgba(255,255,255,.1);--card:#141415}
@media (prefers-color-scheme: light){:root{--bg:#fafafa;--fg:#111;--mut:#6b6b72;--line:rgba(0,0,0,.1);--card:#fff}}
*{box-sizing:border-box}body{margin:0;background:var(--bg);color:var(--fg);font:14px/1.5 Inter,system-ui,sans-serif}
header{padding:28px 32px 8px}h1{margin:0 0 4px;font-size:22px}p{margin:0;color:var(--mut);max-width:70ch}
.grid{display:grid;grid-template-columns:repeat(auto-fill,minmax(360px,1fr));gap:20px;padding:20px 32px 48px}
a.card{display:block;color:inherit;text-decoration:none;border:1px solid var(--line);border-radius:12px;overflow:hidden;background:var(--card)}
a.card:hover{border-color:var(--mut)}
.shot{position:relative;width:100%;aspect-ratio:1282/802;overflow:hidden;border-bottom:1px solid var(--line)}
.shot iframe{position:absolute;left:0;top:0;width:1282px;height:802px;border:0;transform-origin:0 0;pointer-events:none}
.cap{padding:10px 14px;display:flex;justify-content:space-between;gap:10px}.cap span:last-child{color:var(--mut)}
</style></head><body>
<header><h1>acportal desktop: real app screens as HTML</h1>
<p>Each page replicates one screenshot in <code>design/real-app/</code> with the desktop app's own stylesheet and class names. Click a card to open it full size.</p></header>
<div class="grid">__CARDS__</div>
<script>
function fit(){document.querySelectorAll('.shot').forEach(function(s){var f=s.firstElementChild;f.style.transform='scale('+(s.clientWidth/1282)+')'})}
addEventListener('load',fit);addEventListener('resize',fit);fit();
</script></body></html>
"""


def main():
    cards = []
    for name, title, detail, theme, fn in SCREENS:
        out = page(f'{title} · acportal', fn(), theme, detail)
        with open(os.path.join(HERE, name + '.html'), 'w', encoding='utf-8') as f:
            f.write(out)
        cards.append(f'<a class="card" href="{name}.html"><div class="shot"><iframe src="{name}.html" loading="lazy" tabindex="-1" title="{esc(title)}"></iframe></div>'
                     f'<div class="cap"><span>{esc(title)}</span><span>{name[:2]} · {theme}</span></div></a>')
    with open(os.path.join(HERE, 'index.html'), 'w', encoding='utf-8') as f:
        f.write(INDEX.replace('__CARDS__', '\n'.join(cards)))
    print(f'wrote {len(SCREENS)} pages and index.html')


if __name__ == '__main__':
    main()
