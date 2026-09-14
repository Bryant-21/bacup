Function Fragment_Begin(ObjectReference akSpeakerRef)
    Quests:LC129:PumpkinQuestScript pumpkinQuest = GetOwningQuest() as Quests:LC129:PumpkinQuestScript
    If pumpkinQuest != None
        pumpkinQuest.TryTurnInPumpkins()
    EndIf
EndFunction
