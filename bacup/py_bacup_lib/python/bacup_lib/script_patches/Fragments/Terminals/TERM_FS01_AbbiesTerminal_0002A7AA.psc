Function Fragment_Terminal_01(ObjectReference akTerminalRef)
    Actor playerRef = Game.GetPlayer()
    If playerRef != None && pBoS01 != None && BoS01_QuestStartKeyword != None && !pBoS01.IsRunning() && !pBoS01.IsCompleted()
        If pBoS01CompletedAV == None || playerRef.GetValue(pBoS01CompletedAV) < 1.0
            BoS01_QuestStartKeyword.SendStoryEventAndWait(None, playerRef)
        EndIf
    EndIf
EndFunction
