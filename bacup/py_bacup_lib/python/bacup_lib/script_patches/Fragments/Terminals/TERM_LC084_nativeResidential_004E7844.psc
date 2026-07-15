Function SetLinkedRobotsEnabled(ObjectReference akTerminalRef, Bool abEnabled)
    ObjectReference[] linkedRefs = akTerminalRef.GetLinkedRefArray(LinkTerminalRobot)
    Int i = 0
    While i < linkedRefs.Length
        Actor robot = linkedRefs[i] as Actor
        If robot != None && (ActorTypeRobot == None || robot.HasKeyword(ActorTypeRobot))
            robot.StopCombat()
            robot.SetUnconscious(!abEnabled)
            If abEnabled
                robot.SetValue(ProtectronPodStatus, 0.0)
            Else
                robot.SetValue(ProtectronPodStatus, 1.0)
            EndIf
            robot.EvaluatePackage(False)
            Default2StateActivator pod = robot.GetLinkedRef(LinkProtectronPod) as Default2StateActivator
            If pod != None
                pod.IsAnimating = True
                pod.SetOpenNoWait(abEnabled)
            EndIf
        EndIf
        i = i + 1
    EndWhile
EndFunction

Function RemoveLinkedRobotTargetingRestrictions(ObjectReference akTerminalRef)
    ObjectReference[] linkedRefs = akTerminalRef.GetLinkedRefArray(LinkTerminalRobot)
    Actor player = Game.GetPlayer()
    Int i = 0
    While i < linkedRefs.Length
        Actor robot = linkedRefs[i] as Actor
        If robot != None && (ActorTypeRobot == None || robot.HasKeyword(ActorTypeRobot))
            robot.SetUnconscious(False)
            robot.StartCombat(player)
        EndIf
        i = i + 1
    EndWhile
EndFunction

Function Fragment_Terminal_01(ObjectReference akTerminalRef)
    SetLinkedRobotsEnabled(akTerminalRef, False)
EndFunction

Function Fragment_Terminal_02(ObjectReference akTerminalRef)
    SetLinkedRobotsEnabled(akTerminalRef, True)
EndFunction

Function Fragment_Terminal_04(ObjectReference akTerminalRef)
    RemoveLinkedRobotTargetingRestrictions(akTerminalRef)
EndFunction
