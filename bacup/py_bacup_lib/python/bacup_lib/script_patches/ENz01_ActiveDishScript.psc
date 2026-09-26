; Bound to the objective-target copies of each triangulation point. The player
; orienting a point completes its stage once; the paired ENz01_AnimScript on
; the base alias animates the dish.
Event OnActivate(ObjectReference akActionRef)
    Actor playerRef = Game.GetPlayer()
    If akActionRef == None || akActionRef != playerRef
        Return
    EndIf
    Quest owner = GetOwningQuest()
    If owner == None || !owner.IsRunning() || owner.IsStopping() || owner.IsStageDone(iStageToSet)
        Return
    EndIf
    If PlayerActivatedDish != None && PlayerActivatedDish.Find(playerRef) < 0
        PlayerActivatedDish.AddRef(playerRef)
    EndIf
    owner.SetStage(iStageToSet)
EndEvent
