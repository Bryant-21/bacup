Function Fragment_End(Actor akActor)
    ; Exit package for the quest-created Astronaut_Instanced copy once stage 4200 ("Astronaut Leaves") is set.
    Quest owningQuest = GetOwningQuest()
    If akActor && owningQuest && owningQuest.IsStageDone(4200)
        akActor.Disable()
    EndIf
EndFunction
