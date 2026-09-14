Function StartLockdownScene(Scene akScene)
    If akScene != None && !akScene.IsPlaying()
        akScene.Start()
    EndIf
EndFunction

Function Fragment_Terminal_02(ObjectReference akTerminalRef)
    LC006_PoseidonPlantQuestScript questScript = LC006_PoseidonPlant as LC006_PoseidonPlantQuestScript
    If questScript != None
        StartLockdownScene(questScript.LC006_PoseidonPlant_FacilityLockdownStart)
    EndIf
EndFunction

Function Fragment_Terminal_03(ObjectReference akTerminalRef)
    LC006_PoseidonPlantQuestScript questScript = LC006_PoseidonPlant as LC006_PoseidonPlantQuestScript
    If questScript != None
        StartLockdownScene(questScript.LC006_PoseidonPlant_FacilityLockdownEnd)
    EndIf
EndFunction

Function Fragment_Terminal_04(ObjectReference akTerminalRef)
    LC006_PoseidonPlantQuestScript questScript = LC006_PoseidonPlant as LC006_PoseidonPlantQuestScript
    If questScript != None
        StartLockdownScene(questScript.LC006_PoseidonPlant_ReactorLockdownStart)
    EndIf
EndFunction

Function Fragment_Terminal_05(ObjectReference akTerminalRef)
    LC006_PoseidonPlantQuestScript questScript = LC006_PoseidonPlant as LC006_PoseidonPlantQuestScript
    If questScript != None
        StartLockdownScene(questScript.LC006_PoseidonPlant_ReactorLockdownEnd)
    EndIf
EndFunction
