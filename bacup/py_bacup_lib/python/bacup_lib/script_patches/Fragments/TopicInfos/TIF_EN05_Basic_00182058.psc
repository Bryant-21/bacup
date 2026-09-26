Function Fragment_Begin(ObjectReference akSpeakerRef)
    EN05_QuestScript basic = GetOwningQuest() as EN05_QuestScript
    If basic != None
        basic.EN05Basic_BeginIntroduction(True)
    EndIf
EndFunction
