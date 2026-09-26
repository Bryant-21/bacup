Keyword Function GetVendorProxyLink()
    Return Game.GetFormFromFile(0x0005D5E6, "Fallout4.esm") as Keyword
EndFunction

Actor Function GetOrCreateVendorProxy()
    If MTRz05MapMachineFaction == None
        Return None
    EndIf

    Keyword proxyLink = GetVendorProxyLink()
    If proxyLink == None
        Return None
    EndIf

    Actor proxy = GetLinkedRef(proxyLink) as Actor
    If proxy == None
        ActorBase proxyBase = Game.GetFormFromFile(0x001CF4B3, "Fallout4.esm") as ActorBase
        If proxyBase == None
            Return None
        EndIf

        proxy = PlaceAtMe(proxyBase, 1, False, True, False) as Actor
        If proxy != None
            SetLinkedRef(proxy, proxyLink)
        EndIf
    EndIf

    If proxy != None
        proxy.SetAlpha(0.0, False)
        proxy.SetGhost(True)
        proxy.SetRestrained(True)
        proxy.AddToFaction(MTRz05MapMachineFaction)
        proxy.Disable(False)
    EndIf
    Return proxy
EndFunction

Event OnLoad()
    GetOrCreateVendorProxy()
EndEvent

Event OnActivate(ObjectReference akActionRef)
    If akActionRef != Game.GetPlayer() as ObjectReference
        Return
    EndIf

    Actor proxy = GetOrCreateVendorProxy()
    If proxy != None
        Utility.Wait(0.25)
        proxy.ShowBarterMenu()
    EndIf
EndEvent

Event OnUnload()
    If !IsDisabled() && !IsDeleted()
        Return
    EndIf

    Keyword proxyLink = GetVendorProxyLink()
    If proxyLink == None
        Return
    EndIf

    Actor proxy = GetLinkedRef(proxyLink) as Actor
    If proxy != None
        SetLinkedRef(None, proxyLink)
        proxy.Disable(False)
        proxy.Delete()
    EndIf
EndEvent
