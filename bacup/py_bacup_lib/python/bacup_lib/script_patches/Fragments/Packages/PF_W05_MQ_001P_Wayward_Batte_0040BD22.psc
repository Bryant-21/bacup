Function Fragment_End(Actor akActor)
    If !GetOwningQuest().IsStageDone(600)
        GetOwningQuest().SetStage(600)
    EndIf
EndFunction
