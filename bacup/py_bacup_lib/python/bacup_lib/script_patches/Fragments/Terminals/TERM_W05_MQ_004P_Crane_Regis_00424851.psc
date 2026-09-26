Function Fragment_Terminal_01(ObjectReference akTerminalRef)
    Actor playerRef = Game.GetPlayer()
    If playerRef == None || W05_MQ_004P_Crane_PlayerRegisteredPipBoy == None
        Return
    EndIf
    Bool firstRegistration = playerRef.GetValue(W05_MQ_004P_Crane_PlayerRegisteredPipBoy) < 1.0
    playerRef.SetValue(W05_MQ_004P_Crane_PlayerRegisteredPipBoy, 1.0)
    If W05_MQ_004P_Crane != None && W05_MQ_004P_Crane.IsRunning() && !W05_MQ_004P_Crane.IsCompleted()
        If !W05_MQ_004P_Crane.IsStageDone(820)
            W05_MQ_004P_Crane.SetStage(820)
        EndIf
    EndIf
    If W05_MQ_004P_Crane_BunkerQuest != None && W05_MQ_004P_Crane_BunkerQuest.IsRunning()
        W05_MQ_004P_Crane_BunkerQuest.Stop()
    EndIf
    If firstRegistration && W05_MQ_004P_Crane_PipBoyRegistered != None
        W05_MQ_004P_Crane_PipBoyRegistered.Add()
    EndIf
EndFunction
