Actor B21LOSViewer
ObjectReference B21LOSTarget
Bool B21LOSDesired

Function B21RegisterForSingleLOSGain(Actor viewer, ObjectReference target)
    B21LOSViewer = viewer
    B21LOSTarget = target
    B21LOSDesired = True
    StartTimer(0.1, 1)
EndFunction

Function B21RegisterForSingleLOSLost(Actor viewer, ObjectReference target)
    B21LOSViewer = viewer
    B21LOSTarget = target
    B21LOSDesired = False
    StartTimer(0.1, 1)
EndFunction

Function B21CheckLOS()
    If B21LOSViewer.HasDetectionLOS(B21LOSTarget) == B21LOSDesired
        If B21LOSDesired
            Self.OnGainLOS(B21LOSViewer, B21LOSTarget)
        Else
            Self.OnLostLOS(B21LOSViewer, B21LOSTarget)
        EndIf
    Else
        StartTimer(0.1, 1)
    EndIf
EndFunction
