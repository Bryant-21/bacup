Function Fragment_Terminal_03()
    ; "Remove HUMAN class from Target List" completes Activity: Fertile Soil. The
    ; terminal carries no quest property, and the FO76 menu-item conditions that
    ; gated this item on stage 400 did not survive conversion, so the stage check
    ; lives here.
    Quest reaperQuest = Game.GetFormFromFile(0x0000FFED, "SeventySix.esm") as Quest
    If reaperQuest == None || !reaperQuest.IsRunning()
        Return
    EndIf
    If reaperQuest.IsStageDone(400) && !reaperQuest.IsStageDone(500)
        reaperQuest.SetStage(500)
    EndIf
EndFunction
