Event OnBegin(ObjectReference akSpeakerRef, Bool abHasBeenSaid)
    EN05_QuestScript basic = GetOwningQuest() as EN05_QuestScript
    If basic != None && !basic.IsStageDone(iShutdownStage)
        basic.EN05Basic_ShowPowerArmorWarning()
    EndIf
EndEvent
