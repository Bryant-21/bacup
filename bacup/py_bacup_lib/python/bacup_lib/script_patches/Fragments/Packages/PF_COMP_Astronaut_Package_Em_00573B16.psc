Function Fragment_End(Actor akActor)
    ; Exit package for the quest-created Emerson_Instanced copy once stage 4300 ("Emerson Leaves") is set.
    Quest owningQuest = GetOwningQuest()
    If akActor && owningQuest && owningQuest.IsStageDone(4300)
        akActor.Disable()
    EndIf
EndFunction
