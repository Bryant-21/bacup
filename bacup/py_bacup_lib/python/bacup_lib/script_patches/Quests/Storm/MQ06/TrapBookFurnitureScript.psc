Event OnActivate(ObjectReference akActivator)
  if sAnimationEvent == "armed" || sAnimationEvent == "spent"
    return
  endif

  sAnimationEvent = "armed"

  if akActivator != None && RegisterForAnimationEvent(akActivator, "SpawnExplosion.Storm_ExplosionTrapBook")
    StartTimer(4.0, 1)
  else
    Detonate(akActivator)
  endif
EndEvent

Event OnAnimationEvent(ObjectReference akSource, string asEventName)
  if asEventName != "SpawnExplosion.Storm_ExplosionTrapBook"
    return
  endif

  UnregisterForAnimationEvent(akSource, "SpawnExplosion.Storm_ExplosionTrapBook")

  if sAnimationEvent == "armed"
    CancelTimer(1)
    Detonate(akSource)
  endif
EndEvent

Event OnTimer(int aiTimerID)
  if aiTimerID == 1 && sAnimationEvent == "armed"
    Detonate(None)
  endif
EndEvent

Function Detonate(ObjectReference akSource)
  if sAnimationEvent == "spent"
    return
  endif

  sAnimationEvent = "spent"

  if BookExplosion == None
    return
  endif

  ObjectReference explosionRef = None

  if akSource != None
    explosionRef = akSource.PlaceAtNode("RArm_Hand", BookExplosion)
  endif

  if explosionRef == None
    PlaceAtMe(BookExplosion)
  endif
EndFunction
