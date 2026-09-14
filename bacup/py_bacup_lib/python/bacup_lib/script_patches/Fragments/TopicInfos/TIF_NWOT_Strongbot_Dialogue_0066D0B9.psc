Function Fragment_End(ObjectReference akSpeakerRef)
    If PQStartKeyword != None && NukacadePointerQuest != None && !NukacadePointerQuest.IsRunning() && !NukacadePointerQuest.IsCompleted()
        PQStartKeyword.SendStoryEvent(akRef1 = Game.GetPlayer(), akRef2 = akSpeakerRef)
    EndIf
EndFunction
