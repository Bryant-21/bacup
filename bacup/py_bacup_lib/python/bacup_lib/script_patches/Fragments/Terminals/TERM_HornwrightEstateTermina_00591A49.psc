Function Fragment_Terminal_05(ObjectReference akTerminalRef)
    RetirePasscodeChoice(W05_MQS_201P_KeycardClue03Found)
EndFunction

Function Fragment_Terminal_06(ObjectReference akTerminalRef)
    RetirePasscodeChoice(W05_MQS_201P_KeycardClue04Found)
EndFunction

Function Fragment_Terminal_427(ObjectReference akTerminalRef)
    RetireAllPasscodeChoices()
EndFunction

Function Fragment_Terminal_777(ObjectReference akTerminalRef)
    RetireAllPasscodeChoices()
EndFunction

Function RetireAllPasscodeChoices()
    RetirePasscodeChoice(W05_MQS_201P_KeycardClue01Found)
    RetirePasscodeChoice(W05_MQS_201P_KeycardClue02Found)
    RetirePasscodeChoice(W05_MQS_201P_KeycardClue03Found)
    RetirePasscodeChoice(W05_MQS_201P_KeycardClue04Found)
EndFunction

Function RetirePasscodeChoice(ActorValue akClue)
    Actor playerRef = Game.GetPlayer()
    If playerRef != None && akClue != None && playerRef.GetValue(akClue) == 1.0
        playerRef.SetValue(akClue, 2.0)
    EndIf
EndFunction
