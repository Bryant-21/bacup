; Wolf's emergency check-in on the estate terminal points to his cache (300), which is the
; prerequisite of the ARK hiding-place activation that completes the quest.
Function Fragment_Terminal_03(ObjectReference akTerminalRef)
    Quest wolfQuest = Game.GetFormFromFile(0x003FBF2E, "SeventySix.esm") as Quest
    If wolfQuest && wolfQuest.IsRunning() && !wolfQuest.IsStageDone(300)
        wolfQuest.SetStage(300)
    EndIf
EndFunction
