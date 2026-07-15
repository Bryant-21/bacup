Function Fragment_Terminal_01(ObjectReference akTerminalRef)
    (MSilo as MSiloQuestScript_Reactor).ShutdownReactor()
EndFunction

Function Fragment_Terminal_02(ObjectReference akTerminalRef)
    (MSilo as MSiloQuestScript_Reactor).RestartReactor()
EndFunction

Function Fragment_Terminal_04(ObjectReference akTerminalRef)
    (MSilo as MSiloQuestScript_Reactor).ShowRepairInstructions()
EndFunction
