Function Fragment_Begin(ObjectReference akSpeakerRef)
    mtns06questscript eventScript = GetOwningQuest() as mtns06questscript
    Actor playerRef = Game.GetPlayer()
    If eventScript == None || playerRef == None || eventScript.PlayersIdentified == None
        Return
    EndIf
    If eventScript.IsRunning() && !eventScript.IsStageDone(eventScript.ActivityOverSuccessStage) && !eventScript.IsStageDone(eventScript.ActivityOverFailureStage) && eventScript.PlayersIdentified.Find(playerRef) < 0
        eventScript.PlayersIdentified.AddRef(playerRef)
    EndIf
EndFunction
