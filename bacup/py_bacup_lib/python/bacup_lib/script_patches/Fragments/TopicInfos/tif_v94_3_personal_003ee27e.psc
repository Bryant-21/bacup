Function Fragment_End(ObjectReference akSpeakerRef)
    Actor robot = akSpeakerRef as Actor
    If robot != None
        robot.SetValue(V94_3_Pump_RobotHasPlayedSpawnLineValue, 1.0)
        robot.SetValue(V94_3_Pump_RobotAlphaHasRespawnedValue, 1.0)
    EndIf
EndFunction
