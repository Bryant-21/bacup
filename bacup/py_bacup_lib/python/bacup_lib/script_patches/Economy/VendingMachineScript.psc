Bool Function IsSupportedCapsMachine()
    If VendingMachineFaction == None
        Return False
    EndIf

    Faction medicalFaction = Game.GetFormFromFile(0x00175087, "SeventySix.esm") as Faction
    Faction ammoFaction = Game.GetFormFromFile(0x001750A5, "SeventySix.esm") as Faction
    Return VendingMachineFaction == medicalFaction || VendingMachineFaction == ammoFaction
EndFunction

Keyword Function GetVendorProxyLink()
    Return Game.GetFormFromFile(0x0005D5E6, "Fallout4.esm") as Keyword
EndFunction

Actor Function GetOrCreateVendorProxy()
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
        proxy.AddToFaction(VendingMachineFaction)
        proxy.Disable(False)
    EndIf
    Return proxy
EndFunction

Event OnLoad()
    If IsSupportedCapsMachine()
        GetOrCreateVendorProxy()
    EndIf
EndEvent

Event OnActivate(ObjectReference akActionRef)
    Actor player = Game.GetPlayer()
    If akActionRef != player as ObjectReference || !IsSupportedCapsMachine()
        Return
    EndIf

    Actor proxy = GetOrCreateVendorProxy()
    If proxy != None
        Utility.Wait(0.25)
        proxy.ShowBarterMenu()
    EndIf
EndEvent

Event OnUnload()
    If !IsSupportedCapsMachine() || (!IsDisabled() && !IsDeleted())
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
