Function Fragment_Terminal_01(ObjectReference akTerminalRef)
    Actor playerRef = Game.GetPlayer()
    If playerRef == None || pBoS01 == None
        Return
    EndIf
    If pBoS01CompletedAV != None && playerRef.GetValue(pBoS01CompletedAV) >= 1.0
        Return
    EndIf
    If BoS01_QuestStartKeyword != None && !pBoS01.IsRunning() && !pBoS01.IsCompleted()
        BoS01_QuestStartKeyword.SendStoryEventAndWait(None, playerRef)
    EndIf
    ; The Fort Defiance entry is the Camp Venture lead: BoS01 stage 200 ("Read Abby's terminal").
    ; Nothing else sets it, so a quest already started by Coming to Fruition parked on objective 100.
    If pBoS01.IsRunning() && !pBoS01.IsStageDone(200) && !pBoS01.IsStageDone(300)
        pBoS01.SetStage(200)
    EndIf
EndFunction
