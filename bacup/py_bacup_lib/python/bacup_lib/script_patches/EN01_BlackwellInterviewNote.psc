Event OnRead()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        If EN01_Misc_ReadInterviewNotes != None
            playerRef.SetValue(EN01_Misc_ReadInterviewNotes, 1.0)
        EndIf
        If EN01_PlayerKnowsSamBlackwell != None
            playerRef.SetValue(EN01_PlayerKnowsSamBlackwell, 1.0)
        EndIf
    EndIf
    RevealBunkerMarker()
    AdvanceMiscQuest()
EndEvent

Function RevealBunkerMarker()
    If SamsBunkerMarker == None
        Return
    EndIf
    If SamsBunkerMarker.IsDisabled()
        SamsBunkerMarker.EnableNoWait()
    EndIf
    SamsBunkerMarker.AddToMap(True)
EndFunction

Function AdvanceMiscQuest()
    Quest miscQuest = Game.GetFormFromFile(0x000649C5, "SeventySix.esm") as Quest
    If miscQuest == None || !miscQuest.IsRunning()
        Return
    EndIf
    If !miscQuest.IsStageDone(20)
        miscQuest.SetStage(20)
    EndIf
EndFunction
