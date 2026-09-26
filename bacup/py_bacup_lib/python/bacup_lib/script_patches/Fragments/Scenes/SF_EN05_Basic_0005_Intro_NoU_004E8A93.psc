Function Fragment_Begin()
    EN05_QuestScript basic = GetOwningQuest() as EN05_QuestScript
    If basic != None
        basic.EN05Basic_BeginIntroduction(False)
    EndIf
EndFunction
