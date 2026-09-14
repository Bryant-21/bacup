Function SetLogin(ObjectReference akTerminalRef, Float afLoginID)
    If akTerminalRef != None
        akTerminalRef.SetValue(MoM_CryptosTerminalLoginID, afLoginID)
    EndIf
EndFunction

Function Fragment_Terminal_01(ObjectReference akTerminalRef)
    MoMMasterQuestScript masterScript = MoMMaster as MoMMasterQuestScript
    If masterScript != None && masterScript.MoMQuestList.Length > 2
        Quest targetQuest = masterScript.MoMQuestList[2].MoMQuest
        Int stage = masterScript.CONST_MoM01_RegisteredAsInitiate
        If targetQuest != None && !targetQuest.IsRunning() && !targetQuest.IsCompleted()
            Keyword startKeyword = masterScript.MoMQuestList[2].MoMQuestKeyword
            If startKeyword != None
                startKeyword.SendStoryEventAndWait(None, Game.GetPlayer())
            EndIf
        EndIf
        If targetQuest != None && targetQuest.IsRunning() && !targetQuest.IsStageDone(stage)
            targetQuest.SetStage(stage)
        EndIf
    EndIf
EndFunction

Function Fragment_Terminal_02(ObjectReference akTerminalRef)
    SetLogin(akTerminalRef, 2.0)
    MoMMasterQuestScript masterScript = MoMMaster as MoMMasterQuestScript
    If masterScript != None && masterScript.MoMQuestList.Length > 2
        Quest targetQuest = masterScript.MoMQuestList[2].MoMQuest
        If targetQuest != None && targetQuest.IsStageDone(80) && !targetQuest.IsStageDone(masterScript.CONST_MoM01_ShouldAuthorizePromotion)
            targetQuest.SetStage(masterScript.CONST_MoM01_ShouldAuthorizePromotion)
        EndIf
    EndIf
EndFunction

Function Fragment_Terminal_03(ObjectReference akTerminalRef)
    SetLogin(akTerminalRef, 3.0)
EndFunction

Function Fragment_Terminal_04(ObjectReference akTerminalRef)
    SetLogin(akTerminalRef, 4.0)
EndFunction

Function Fragment_Terminal_05(ObjectReference akTerminalRef)
    SetLogin(akTerminalRef, 5.0)
EndFunction
