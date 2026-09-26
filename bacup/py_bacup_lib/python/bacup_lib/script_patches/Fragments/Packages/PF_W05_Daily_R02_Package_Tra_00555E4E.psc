Function Fragment_End(Actor akActor)
    ; Stage 1330: the quest-created target flees Appalachia via the exit-map travel package.
    Quest owningQuest = GetOwningQuest()
    If akActor && owningQuest && owningQuest.IsStageDone(1330)
        akActor.Disable()
    EndIf
EndFunction
