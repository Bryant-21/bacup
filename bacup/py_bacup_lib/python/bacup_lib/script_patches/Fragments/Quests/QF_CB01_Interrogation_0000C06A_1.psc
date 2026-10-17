; The robot's race supplies the death explosion.
Function Fragment_Stage_0999_Item_00()
    Actor trialRobot = Alias_Robot.GetActorReference()
    If trialRobot != None
        trialRobot.Kill()
    EndIf
EndFunction
