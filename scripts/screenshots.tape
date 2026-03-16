Output screenshots/demo.gif
Set FontSize 14
Set Width 130
Set Height 40
Set Padding 10
Set Shell "bash"

# Launch TUI with sample data
Type "./learner-tui/target/release/solvable ./scripts/sample_data/learnings.db ./scripts/sample_data/research.db"
Enter
Sleep 3s

# Dismiss Welcome screen
Enter
Sleep 2s

# Tab 1: Learnings
Type "1"
Sleep 2s
Screenshot screenshots/01-learnings.png

# Tab 2: Research
Type "2"
Sleep 2s
Screenshot screenshots/02-research.png

# Tab 3: Issues
Type "3"
Sleep 2s
Screenshot screenshots/03-issues.png

# Tab 4: Solutions
Type "4"
Sleep 2s
Screenshot screenshots/04-solutions.png

# Tab 5: Confluence
Type "5"
Sleep 2s
Screenshot screenshots/05-confluence.png

# Tab 6: Solve
Type "6"
Sleep 2s
Screenshot screenshots/06-solve.png

# Tab 7: Portal
Type "7"
Sleep 2s
Screenshot screenshots/07-portal.png

# Tab 8: Settings
Type "8"
Sleep 2s
Screenshot screenshots/08-settings.png

# Return to Learnings and quit
Type "1"
Sleep 2s

Type "q"
