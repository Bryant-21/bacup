Function StartMission(Int aiQuestIndex, Int aiStage)
    MoMMasterQuestScript masterScript = MoMMaster as MoMMasterQuestScript
    If masterScript != None && masterScript.MoMQuestList.Length > aiQuestIndex
        Quest targetQuest = masterScript.MoMQuestList[aiQuestIndex].MoMQuest
        If targetQuest != None && !targetQuest.IsRunning() && !targetQuest.IsCompleted()
            Keyword startKeyword = masterScript.MoMQuestList[aiQuestIndex].MoMQuestKeyword
            If startKeyword != None
                startKeyword.SendStoryEventAndWait(None, Game.GetPlayer())
            EndIf
        EndIf
        If targetQuest != None && targetQuest.IsRunning() && !targetQuest.IsStageDone(aiStage)
            targetQuest.SetStage(aiStage)
        EndIf
    EndIf
EndFunction

Function Fragment_Terminal_01(ObjectReference akTerminalRef)
    MoMMasterQuestScript masterScript = MoMMaster as MoMMasterQuestScript
    If masterScript != None
        StartMission(4, 20)
    EndIf
EndFunction

Function Fragment_Terminal_02(ObjectReference akTerminalRef)
    MoMMasterQuestScript masterScript = MoMMaster as MoMMasterQuestScript
    If masterScript != None
        StartMission(5, 20)
    EndIf
EndFunction

Function Fragment_Terminal_03(ObjectReference akTerminalRef)
    MoMMasterQuestScript masterScript = MoMMaster as MoMMasterQuestScript
    If masterScript != None
        StartMission(6, 20)
    EndIf
EndFunction

Function Fragment_Terminal_04(ObjectReference akTerminalRef)
    MoMMasterQuestScript masterScript = MoMMaster as MoMMasterQuestScript
    If masterScript != None
        StartMission(7, masterScript.CONST_MOM03_AcceptedPleasantValleyMission)
    EndIf
EndFunction
