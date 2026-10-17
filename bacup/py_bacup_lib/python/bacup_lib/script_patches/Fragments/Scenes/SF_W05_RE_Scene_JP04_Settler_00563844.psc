; The captive stays on the HostageFurniture (NPCPrisonerFloorSit, linked via
; LinkCustom03) through a sit package that holds while quest stage 20 ("Remove
; settler hood") is not done; the BoundCaptive responses set stage 20. Each
; phase end re-evaluates the captive so it rises as soon as it is freed.
Function Fragment_Phase_02_End()
    Actor captive = CaptiveRef.GetActorReference()
    If captive != None
        captive.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Phase_03_End()
    Actor captive = CaptiveRef.GetActorReference()
    If captive != None
        captive.EvaluatePackage()
    EndIf
EndFunction
