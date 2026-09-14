Function Fragment_Terminal_01(ObjectReference akTerminalRef)
    If ControlRef != None && ControlRef.RandomDistantSound != None && ControlRef.RandomDistantSound.Length > 0
        ObjectReference[] speakers = ControlRef.GetRefsLinkedToMe(ControlRef.LC101LoudspeakerKeyword, None)
        If speakers != None && speakers.Length > 0 && speakers[0] != None
            ControlRef.RandomDistantSound[0].Play(speakers[0])
        EndIf
    EndIf
EndFunction

Function Fragment_Terminal_02(ObjectReference akTerminalRef)
    If ControlRef != None
        ObjectReference[] pictures = ControlRef.GetRefsLinkedToMe(ControlRef.LC101PictureFrameKeyword, None)
        Int i = 0
        While i < pictures.Length
            If pictures[i] != None
                pictures[i].PlayAnimation("Play01")
            EndIf
            i += 1
        EndWhile
    EndIf
EndFunction

Function Fragment_Terminal_03(ObjectReference akTerminalRef)
    If ControlRef != None
        If ControlRef.LureQuestKeyword != None
            ControlRef.LureQuestKeyword.SendStoryEventAndWait(None, Game.GetPlayer())
        EndIf
        ControlRef.ClientPlayLureEnemiesSound()
    EndIf
EndFunction

Function Fragment_Terminal_04(ObjectReference akTerminalRef)
    If ControlRef != None
        ControlRef.ClientSimulateEarthquake()
    EndIf
EndFunction

Function Fragment_Terminal_05(ObjectReference akTerminalRef)
    If ControlRef != None
        ObjectReference[] doors = ControlRef.GetRefsLinkedToMe(ControlRef.LC101DoorKeyword, None)
        Int i = 0
        While i < doors.Length
            Default2StateActivator door = doors[i] as Default2StateActivator
            If door != None
                door.SetOpen(False)
            EndIf
            i += 1
        EndWhile
    EndIf
EndFunction
