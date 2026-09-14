Function Fragment_Terminal_01(ObjectReference akTerminalRef)
    Actor playerRef = Game.GetPlayer()
    If playerRef != None && BoS01 != None && BoS01_QuestStartKeyword != None && !BoS01.IsRunning() && !BoS01.IsCompleted()
        If BoS01CompletedAV == None || playerRef.GetValue(BoS01CompletedAV) < 1.0
            BoS01_QuestStartKeyword.SendStoryEventAndWait(None, playerRef)
        EndIf
    EndIf
EndFunction
