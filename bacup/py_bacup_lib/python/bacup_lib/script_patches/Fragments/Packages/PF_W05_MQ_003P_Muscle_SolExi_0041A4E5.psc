Function Fragment_End(Actor akActor)
    Quest owningQuest = GetOwningQuest()
    If akActor != None && owningQuest != None && owningQuest.IsRunning() && owningQuest.IsStageDone(500)
        akActor.DisableNoWait()
    EndIf
EndFunction
