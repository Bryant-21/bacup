; Dining chairs (Furn_Chairs) report the player sitting down and standing up to the waiter quest.
Event OnActivate(ObjectReference akSenderRef, ObjectReference akActionRef)
    Actor diner = akActionRef as Actor
    If diner == None || diner != Game.GetPlayer() || lock_DiningRoomChairState
        Return
    EndIf
    lock_DiningRoomChairState = True
    LC060_WhitespringDiningRoomScript diningRoom = GetOwningQuest() as LC060_WhitespringDiningRoomScript
    If diningRoom != None
        diningRoom.AssignWaiter(diner, akSenderRef)
    EndIf
    lock_DiningRoomChairState = False
EndEvent

Event OnExitFurniture(ObjectReference akSenderRef, ObjectReference akActionRef)
    Actor diner = akActionRef as Actor
    LC060_WhitespringDiningRoomScript diningRoom = GetOwningQuest() as LC060_WhitespringDiningRoomScript
    If diner != None && diningRoom != None
        diningRoom.ReleaseDiner(diner)
    EndIf
EndEvent
