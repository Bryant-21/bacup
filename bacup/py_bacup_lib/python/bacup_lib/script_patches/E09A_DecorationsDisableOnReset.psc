; E09A event decoration enable markers start hidden again when their cell resets outside the event; while
; E09A_Launcher 63461B runs, E09A_QuestScript.SetDecorationsEnabled owns them.
Event OnReset()
    Quest launcher = Game.GetFormFromFile(0x0063461B, "SeventySix.esm") as Quest
    If launcher == None || !launcher.IsRunning()
        Disable()
    EndIf
EndEvent
