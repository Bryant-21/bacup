Event OnActivate(ObjectReference akActionRef)
    ObjectReference bookshelfRef = GetLinkedRef(bookshelfKeyword)
    ObjectReference navcutRef = GetLinkedRef(navcutKeyword)

    If bookshelfRef != None
        bookshelfRef.DisableNoWait()
    EndIf
    If navcutRef != None
        navcutRef.DisableNoWait()
    EndIf
EndEvent
