Function Fragment_Terminal_01(ObjectReference akTerminalRef)
    If Game.GetPlayer().GetValue(W05_MQ_004P_Crane_PlayerRegisteredPipBoy) != 0.0
        Return
    EndIf
    Game.GetPlayer().SetValue(W05_MQ_004P_Crane_PlayerRegisteredPipBoy, 1.0)
    W05_MQ_004P_Crane.SetCurrentStageID(820)
    W05_MQ_004P_Crane_BunkerQuest.Stop()
    W05_MQ_004P_Crane_PipBoyRegistered.Add()
EndFunction
